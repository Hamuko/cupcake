mod data;
mod utils;

use chrono::Utc;
use clap::Parser;
use futures_util::FutureExt;
use rust_socketio::asynchronous::ClientBuilder;
use rust_socketio::{Payload, TransportType};
use serde_json::json;
use simple_logger::SimpleLogger;
use std::fs::File;
use std::io::prelude::*;
use std::sync::Arc;
use tokio::signal;
use tokio::time::Duration;
use tokio::{sync::mpsc, time::timeout};

const BUFFER_COUNT: usize = 64;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

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
}

#[derive(Debug)]
enum Event {
    Chat(Vec<serde_json::value::Value>),
    Disconnect,
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
    File::create(filename).expect("Could not create output file")
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
    for server in socket_config.servers {
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

    let notify = Arc::new(tokio::sync::Notify::new());
    let notify_clone = notify.clone();

    let socket = ClientBuilder::new(socket_address)
        .transport_type(TransportType::Any)
        .on(rust_socketio::Event::Connect, move |_, _| {
            let cl = notify_clone.clone();
            async move {
                log::info!("Connected to server");
                cl.notify_one();
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
                log::error!("Received error: {:#?}", err);
            }
            .boxed()
        })
        .on("chatMsg", move |payload, _| {
            let tx_ = chat_tx.clone();
            async move {
                if let Payload::Text(values) = payload {
                    tx_.send(Event::Chat(values))
                        .await
                        .expect("Could not send chat message to channel");
                }
            }
            .boxed()
        })
        .connect()
        .await
        .expect("Connection failed");

    timeout(CONNECT_TIMEOUT, notify.notified())
        .await
        .expect("Timed out connecting");

    match socket
        .emit("joinChannel", json!({"name": args.channel}))
        .await
    {
        Ok(_) => log::info!("Joined channel {}", args.channel),
        Err(e) => {
            log::error!("Could not join channel {}: {}", args.channel, e);
            return;
        }
    };

    let manager = tokio::spawn(async move {
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
                        match write!(&mut file, "{}\n", chat) {
                            Ok(_) => log::debug!("{}", chat),
                            Err(e) => log::warn!("Failed to write '{}' to file: {}", chat, e),
                        };
                    }
                }
                Event::Disconnect => {
                    log::warn!("Client disconnected from server");
                    // break;
                }
                Event::Terminate => {
                    log::info!("Terminating cupcake");
                    break;
                }
            }
        }
    });

    // Wait for SIGINT (Ctrl-C) to end the client.
    match signal::ctrl_c().await {
        Ok(()) => log::debug!("Received Ctrl+C"),
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
