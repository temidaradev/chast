use anyhow::Result;
use futures_lite::StreamExt;
use iroh::{Endpoint, protocol::Router};
use iroh_gossip::{api::Event, net::Gossip, proto::TopicId};
use messaging::{Message, MessageBody, StoredMessage};
use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Arc, Mutex},
};
use ticket::Ticket;

const DEFAULT_RELAY_URL: &str = "https://relay.iroh.link";
const MESSAGE_HISTORY_FILE: &str = "server_message_history.json";

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starting Chat Server...");

    let topic = TopicId::from_bytes(rand::random());
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;
    let node_id = endpoint.node_id();

    println!("Server Node ID: {}", node_id);

    let gossip = Gossip::builder().spawn(endpoint.clone());
    let _router = Router::builder(endpoint.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn();

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let mut node_addr = iroh::NodeAddr::new(endpoint.node_id());
    if let Some(_discovery) = endpoint.discovery() {
        let relay_url = DEFAULT_RELAY_URL.parse().expect("Invalid relay URL");
        node_addr = node_addr.with_relay_url(relay_url);
    }

    let ticket = Ticket {
        topic,
        nodes: vec![node_addr],
    };

    println!("Room created successfully!");
    println!("Share this ticket with others to join:");
    println!("{}", "=".repeat(60));
    println!("{}", ticket);
    println!("{}", "=".repeat(60));
    println!("Server is running. Others can join using the p2p-chat app with this ticket.");

    let subscription = gossip.subscribe(topic, vec![]).await?;
    let (sender, mut receiver) = subscription.split();

    let users: Arc<Mutex<HashMap<iroh::NodeId, String>>> = Arc::new(Mutex::new(HashMap::new()));
    users.lock().unwrap().insert(node_id, "Server".to_string());

    let message_history: Arc<Mutex<Vec<StoredMessage>>> =
        Arc::new(Mutex::new(load_message_history()));

    println!("Chat log will appear below:");
    println!("{}", "-".repeat(60));

    let existing_count = message_history.lock().unwrap().len();
    if existing_count > 0 {
        println!("Loaded {} existing messages from history", existing_count);
    }

    loop {
        if let Ok(Some(event)) = receiver.try_next().await {
            if let Event::Received(msg) = event {
                if let Ok(message) = Message::from_bytes(&msg.content) {
                    match message.body {
                        MessageBody::AboutMe { from, name } => {
                            let is_new_user = !users.lock().unwrap().contains_key(&from);
                            users.lock().unwrap().insert(from, name.clone());

                            if is_new_user {
                                println!(
                                    "{} joined the room ({})",
                                    name,
                                    from.to_string().chars().take(8).collect::<String>()
                                );

                                let user_count = users.lock().unwrap().len();
                                println!("{} users online", user_count);

                                let history = message_history.lock().unwrap().clone();
                                if !history.is_empty() {
                                    let history_message =
                                        Message::new(MessageBody::MessageHistory {
                                            messages: history,
                                        });

                                    if let Err(e) =
                                        sender.broadcast(history_message.to_vec().into()).await
                                    {
                                        eprintln!("Failed to send message history: {}", e);
                                    } else {
                                        println!(
                                            "Sent {} messages from history to {}",
                                            message_history.lock().unwrap().len(),
                                            name
                                        );
                                    }
                                }
                            }
                        }
                        MessageBody::Message { from, text } => {
                            let sender_name = users
                                .lock()
                                .unwrap()
                                .get(&from)
                                .cloned()
                                .unwrap_or_else(|| {
                                    format!(
                                        "User-{}",
                                        from.to_string().chars().take(8).collect::<String>()
                                    )
                                });

                            let timestamp =
                                chrono::Local::now().format("%d/%m/%Y %H:%M:%S").to_string();
                            println!("[{}] {}: {}", timestamp, sender_name, text);

                            let stored_message = StoredMessage {
                                from,
                                sender_name: sender_name.clone(),
                                text: text.clone(),
                                timestamp: timestamp.clone(),
                            };

                            message_history.lock().unwrap().push(stored_message);

                            if let Err(e) = save_message_history(&message_history.lock().unwrap()) {
                                eprintln!("Failed to save message history: {}", e);
                            }
                        }
                        MessageBody::Disconnect { from, name } => {
                            users.lock().unwrap().remove(&from);
                            println!("{} left the room", name);

                            let user_count = users.lock().unwrap().len();
                            println!("{} users online", user_count);
                        }
                        MessageBody::MessageHistory { .. } => {
                            // Server doesn't need to process history messages sent by itself
                        }
                    }
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

fn load_message_history() -> Vec<StoredMessage> {
    if Path::new(MESSAGE_HISTORY_FILE).exists() {
        match fs::read_to_string(MESSAGE_HISTORY_FILE) {
            Ok(content) => match serde_json::from_str::<Vec<StoredMessage>>(&content) {
                Ok(messages) => messages,
                Err(e) => {
                    eprintln!("Failed to parse message history: {}", e);
                    Vec::new()
                }
            },
            Err(e) => {
                eprintln!("Failed to read message history file: {}", e);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    }
}

fn save_message_history(messages: &[StoredMessage]) -> Result<()> {
    let json = serde_json::to_string_pretty(messages)?;
    fs::write(MESSAGE_HISTORY_FILE, json)?;
    Ok(())
}
