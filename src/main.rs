mod data;
mod utils;

use chrono::Utc;
use clap::Parser;
use futures_util::FutureExt;
use rust_socketio::asynchronous::{Client, ClientBuilder};
use rust_socketio::{Payload, TransportType};
use serde_json::{json, Value};
use simple_logger::SimpleLogger;
use std::fs::File;
use std::io::prelude::*;
use tokio::signal;
use tokio::sync::mpsc;

const BUFFER_COUNT: usize = 64;

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Cytube server domain.
    #[clap(value_parser = utils::parse_domain)]
    domain: url::Host,

    /// Cytube channel name.
    channel: String,

    /// Application logging level.
    #[clap(long, default_value_t = log::LevelFilter::Info)]
    log_level: log::LevelFilter,

    /// Join as guest with the given name.
    /// This prevents receiving messages from shadowbanned users.
    #[clap(long)]
    guest_login: Option<String>,
}

#[derive(Debug)]
enum Event {
    Chat(Vec<Value>),
    Disconnect,
    Login(Vec<Value>),
    Terminate,
}

enum SocketAddressError {
    NotFound,
    Parse(serde_json::Error),
    Request(reqwest::Error),
}

fn create_chat_log_file(channel: &str) -> File {
    let filename = format!(
        "chat-{}-{}Z.txt",
        channel,
        Utc::now().format("%Y%m%dT%H%M%S")
    );
    let file = File::create(&filename).expect("Could not create output file");
    log::info!("Created chat log file {}", filename);
    file
}

fn handle_login_event(values: Vec<Value>) {
    for value in values {
        let login: data::Login = match serde_json::from_value(value) {
            Ok(v) => v,
            Err(e) => {
                log::error!("Could not parse login payload: {}", e);
                continue;
            }
        };

        if login.success {
            log::info!(
                "Logged in as guest {}",
                login.name.unwrap_or("Unknown".into())
            );
        } else {
            log::warn!(
                "Login failed: {}",
                login.error.unwrap_or("Unknown error".into())
            );
        }
    }
}

/// Join a channel on the Cytube server.
async fn join_channel(client: &Client, channel_name: &str) {
    match client
        .emit("joinChannel", json!({"name": channel_name}))
        .await
    {
        Ok(_) => log::info!("Joined channel {}", channel_name),
        Err(e) => {
            log::error!("Could not join channel {}: {}", channel_name, e);
        }
    };
}

/// Login as a guest user on the Cytube server.
async fn login_as_guest(client: &Client, name: &str) {
    match client.emit("login", json!({"name": name})).await {
        Ok(_) => log::debug!("Login request sent"),
        Err(e) => {
            log::error!("Could not send login request: {}", e);
        }
    };
}

/// Fetch Cytube socket config and return the URL of the first Socket.IO server.
async fn lookup_socket_address(
    domain: &url::Host,
    channel: &str,
) -> Result<String, SocketAddressError> {
    log::info!("Looking up socket address...");
    let url = format!("https://{}/socketconfig/{}.json", domain, channel);
    log::debug!("Fetching socket config from {}", url);
    let response = reqwest::get(&url)
        .await
        .map_err(SocketAddressError::Request)?;
    let content = response.text().await.map_err(SocketAddressError::Request)?;
    let socket_config: data::SocketConfig =
        serde_json::from_str(&content).map_err(SocketAddressError::Parse)?;
    if let Some(server) = socket_config.servers.into_iter().next() {
        log::info!("Found {}", server.url);
        return Ok(server.url);
    }
    Err(SocketAddressError::NotFound)
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    SimpleLogger::new()
        .with_level(args.log_level)
        .env()
        .init()
        .unwrap();

    // Convert Cytube domain and channel name to socket address.
    let socket_address = match lookup_socket_address(&args.domain, &args.channel).await {
        Ok(address) => address,
        Err(err) => {
            match err {
                SocketAddressError::NotFound => {
                    log::error!("Failed to find socket address in Cytube socket config");
                }
                SocketAddressError::Request(e) => {
                    log::error!("Failed to fetch Cytube socket config: {}", e);
                }
                SocketAddressError::Parse(e) => {
                    log::error!("Failed to parse Cytube socket config: {}", e);
                }
            }
            std::process::exit(1);
        }
    };

    let mut file = create_chat_log_file(&args.channel);

    let (tx, mut rx) = mpsc::channel(BUFFER_COUNT);
    let chat_tx = tx.clone();
    let disconnect_tx = tx.clone();
    let login_tx = tx.clone();

    let socket = ClientBuilder::new(socket_address)
        .transport_type(TransportType::Any)
        .on(rust_socketio::Event::Connect, move |_, client| {
            let channel_name = args.channel.clone();
            let guest_login = args.guest_login.clone();
            async move {
                log::info!("Connected to server");
                join_channel(&client, &channel_name).await;
                if let Some(username) = guest_login {
                    login_as_guest(&client, &username).await;
                }
            }
            .boxed()
        })
        .on(rust_socketio::Event::Close, move |payload, _| {
            let tx_ = disconnect_tx.clone();
            async move {
                log::warn!("Disconnect: {:?}", payload);
                tx_.send(Event::Disconnect)
                    .await
                    .expect("Could not send disconnect to channel");
            }
            .boxed()
        })
        .on("error", |err, _| {
            async move {
                match err {
                    Payload::Text(values) => {
                        for value in values {
                            log::error!("Received error: {}", value);
                        }
                    }
                    other => {
                        log::error!("Received error: {:?}", other);
                    }
                }
            }
            .boxed()
        })
        .on("chatMsg", move |payload, _| {
            let tx_ = chat_tx.clone();
            async move {
                if let Payload::Text(values) = payload {
                    tx_.send(Event::Chat(values))
                        .await
                        .expect("Could not send chat payload to channel");
                }
            }
            .boxed()
        })
        .on("login", move |payload, _| {
            let tx_ = login_tx.clone();
            async move {
                if let Payload::Text(values) = payload {
                    tx_.send(Event::Login(values))
                        .await
                        .expect("Could not send login payload to channel");
                }
            }
            .boxed()
        })
        .connect()
        .await
        .expect("Connection failed");

    let manager = tokio::spawn(async move {
        let mut last_timestamp: u64 = 0;
        while let Some(event) = rx.recv().await {
            match event {
                Event::Chat(values) => {
                    for value in values {
                        let chat: data::ChatMessage = match serde_json::from_value(value) {
                            Ok(v) => v,
                            Err(e) => {
                                log::error!("Could not parse chat message: {}", e);
                                continue;
                            }
                        };

                        // Ignore past messages in case of reconnects.
                        if last_timestamp >= chat.time {
                            continue;
                        }
                        last_timestamp = chat.time;

                        match writeln!(&mut file, "{}", chat) {
                            Ok(_) => log::debug!("{}", chat),
                            Err(e) => log::warn!("Failed to write '{}' to file: {}", chat, e),
                        };
                    }
                }
                Event::Disconnect => {
                    log::warn!("Client disconnected from server");
                }
                Event::Login(values) => handle_login_event(values),
                Event::Terminate => {
                    log::info!("Terminating cupcake");
                    break;
                }
            }
        }
    });

    // Wait for SIGINT (Ctrl-C) to end the client.
    match signal::ctrl_c().await {
        Ok(()) => log::debug!("Received SIGINT"),
        Err(err) => {
            log::error!("Unable to listen to shutdown signal: {}", err);
        }
    }
    if let Err(e) = tx.send(Event::Terminate).await {
        log::error!("Could not send termination signal: {}", e);
    }

    manager.await.unwrap();

    // Disconnect the WebSocket client and end the file.
    log::info!("Disconnecting client");
    socket
        .disconnect()
        .await
        .expect("Failed to disconnect from server");
}
