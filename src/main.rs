mod channel;
mod data;
mod utils;

use chrono::Utc;
use clap::Parser;
use futures_util::FutureExt;
use rust_socketio::asynchronous::{Client, ClientBuilder};
use rust_socketio::{Payload, TransportType};
use serde_json::{Value, json};
use simple_logger::SimpleLogger;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::select;
use tokio::signal;
use tokio::time::{Duration, sleep};
use tokio_util::sync::CancellationToken;

const WRITE_BUFFER_SIZE: usize = 8 * 1024; // 8 KiB

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Cytube server domain.
    #[clap(value_parser = utils::parse_domain)]
    domain: url::Host,

    /// Cytube channel name.
    channel: String,

    /// Application logging level.
    #[clap(long, value_name = "LEVEL", default_value_t = log::LevelFilter::Info, env = "CUPCAKE_LOG_LEVEL")]
    log_level: log::LevelFilter,

    /// Join as guest with the given name.
    ///
    /// This prevents receiving messages from shadow-banned users and
    /// makes cupcake visible in the cytube channel's member list.
    /// Username must be unique and non-registered for the option to work.
    #[clap(long, value_name = "USERNAME", env = "CUPCAKE_GUEST_LOGIN")]
    guest_login: Option<String>,

    /// Rotate the chat log file after a certain number of hours.
    #[clap(long, value_name = "HOURS", env = "CUPCAKE_ROTATE_FILE")]
    rotate_file: Option<u64>,
}

#[derive(Debug)]
enum Event {
    Chat(Vec<Value>),
    Disconnect,
    Login(Vec<Value>),
    RotateLog,
    Terminate,
}

enum LoginError {
    Throttled(u64),
    Unknown(Option<String>),
}

enum SocketAddressError {
    NotFound,
    Parse(serde_json::Error),
    Request(reqwest::Error),
}

async fn create_chat_log_file(channel: &str) -> File {
    let filename = format!(
        "chat-{}-{}Z.txt",
        channel,
        Utc::now().format("%Y%m%dT%H%M%S")
    );
    let file = File::create(&filename)
        .await
        .expect("Could not create output file");
    log::info!("Created chat log file {}", filename);
    file
}

fn handle_login_event(values: Vec<Value>) -> Result<(), LoginError> {
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
                login.name.unwrap_or("<UNKNOWN>".into())
            );
            return Ok(());
        } else {
            if let Some(error_message) = login.error {
                if let Some(duration) = utils::check_throttling_error(&error_message) {
                    return Err(LoginError::Throttled(duration));
                } else {
                    return Err(LoginError::Unknown(Some(error_message)));
                }
            }
            return Err(LoginError::Unknown(None));
        }
    }
    Err(LoginError::Unknown(None))
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

/// Periodically send a log rotation event to the main task.
async fn rotate_file_loop(token: CancellationToken, tx: channel::EventTx, hours: u64) {
    let rotate_interval = Duration::from_secs(hours * 60 * 60);
    let mut interval = tokio::time::interval(rotate_interval);
    interval.tick().await;
    loop {
        select! {
            _ = token.cancelled() => {
                log::debug!("Ending log rotation task");
                break;
            },
            _ = interval.tick() => {
                log::debug!("Log rotation interval reached");
                if let Err(err) = tx.send(Event::RotateLog).await {
                    log::error!("Failed to send rotate log event: {}", err);
                }
            }
        }
    }
}

#[cfg(unix)]
/// Wait for SIGINT (Ctrl-C) or SIGTERM to end the client (Unix).
async fn wait_termination() {
    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
        .expect("Could not register SIGTERM handler");
    select! {
        _ = signal::ctrl_c() => log::debug!("Received SIGINT"),
        _ = sigterm.recv() => log::debug!("Received SIGTERM"),
    }
}

#[cfg(not(unix))]
/// Wait for SIGINT (Ctrl-C) to end the client (Windows).
async fn wait_termination() {
    match signal::ctrl_c().await {
        Ok(()) => log::debug!("Received SIGINT"),
        Err(err) => {
            log::error!("Unable to listen to shutdown signal: {}", err);
        }
    }
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

    let file = create_chat_log_file(&args.channel).await;
    let mut file_buffer = BufWriter::with_capacity(WRITE_BUFFER_SIZE, file);

    let (tx, mut rx) = channel::mpsc_channel();

    let chat_tx = tx.clone();
    let disconnect_tx = tx.clone();
    let login_tx = tx.clone();

    // Set up log rotation if --rotate-file is used.
    let cancellation_token = CancellationToken::new();
    let rotate_task = match args.rotate_file {
        Some(hours) => {
            let future = rotate_file_loop(cancellation_token.clone(), tx.clone(), hours);
            Some(tokio::spawn(future))
        }
        None => None,
    };

    let channel_name = args.channel.clone();
    let guest_login = Arc::new(args.guest_login);
    let connect_guest_login = guest_login.clone();
    let socket = ClientBuilder::new(socket_address)
        .transport_type(TransportType::Any)
        .on(rust_socketio::Event::Connect, move |_, client| {
            let channel_name = channel_name.clone();
            let username = connect_guest_login.clone();
            async move {
                log::info!("Connected to server");
                join_channel(&client, &channel_name).await;
                if let Some(username) = &*username {
                    login_as_guest(&client, username).await;
                }
            }
            .boxed()
        })
        .on(rust_socketio::Event::Close, move |payload, _| {
            let tx_ = disconnect_tx.clone();
            async move {
                match payload {
                    Payload::Text(values) => {
                        for value in values {
                            log::warn!("Disconnect: {:?}", value);
                        }
                    }
                    other => {
                        log::warn!("Disconnect: {:?}", other);
                    }
                }
                let _ = tx_.send(Event::Disconnect).await;
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

    let channel_name = args.channel.clone();
    let manager_socket = socket.clone();
    let manager = tokio::spawn(async move {
        let mut last_timestamp: u64 = 0;
        while let Some(event) = channel::read_event(&mut rx).await {
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

                        // Reconnecting makes the server return the last N messages, meaning
                        // that messages may be duplicated if we don't ignore old timestamps.
                        if last_timestamp >= chat.time {
                            continue;
                        }
                        last_timestamp = chat.time;

                        // Ignore special messages.
                        if chat.should_be_skipped() {
                            log::debug!("Ignoring message: {}", chat.short_format());
                            continue;
                        }

                        match file_buffer
                            .write_all(format!("{}\n", chat).as_bytes())
                            .await
                        {
                            Ok(_) => log::debug!("{}", chat),
                            Err(e) => {
                                log::warn!("Failed to write '{}' to file buffer: {}", chat, e)
                            }
                        };
                    }
                }
                Event::Disconnect => {
                    log::warn!("Client disconnected from server");
                    break;
                }
                Event::Login(values) => match handle_login_event(values) {
                    Ok(_) => {}
                    Err(LoginError::Throttled(seconds)) => {
                        log::warn!("Guest login throttled; retrying in {} seconds", seconds);
                        let username = guest_login.clone();
                        let socket = manager_socket.clone();
                        let _ = tokio::spawn(async move {
                            sleep(Duration::from_secs(seconds)).await;
                            if let Some(username) = &*username {
                                login_as_guest(&socket, username).await;
                            }
                        });
                    }
                    Err(LoginError::Unknown(Some(message))) => {
                        log::warn!("Login failed: {}", message)
                    }
                    Err(LoginError::Unknown(None)) => log::warn!("Login failed"),
                },
                Event::RotateLog => {
                    log::info!("Rotating log file...");
                    match file_buffer.flush().await {
                        Ok(()) => log::debug!("File buffer flushed"),
                        Err(e) => log::error!("Failed to flush file buffer: {}", e),
                    };
                    let file = create_chat_log_file(&channel_name).await;
                    file_buffer = BufWriter::with_capacity(WRITE_BUFFER_SIZE, file);
                }
                Event::Terminate => {
                    log::info!("Terminating cupcake");
                    cancellation_token.cancel();
                    break;
                }
            }
        }
        match file_buffer.flush().await {
            Ok(()) => log::debug!("File buffer flushed"),
            Err(e) => log::error!("Failed to flush file buffer: {}", e),
        }
    });

    wait_termination().await;
    if let Err(e) = tx.send(Event::Terminate).await {
        log::error!("Could not send termination signal: {}", e);
    }

    manager.await.unwrap();
    if let Some(rotate_task) = rotate_task {
        rotate_task.await.unwrap();
    }

    // Disconnect the WebSocket client.
    log::info!("Disconnecting client");
    socket
        .disconnect()
        .await
        .expect("Failed to disconnect from server");
}
