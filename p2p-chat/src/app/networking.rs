use anyhow::Result;
use futures_lite::StreamExt;
use iroh::{Endpoint, NodeId, protocol::Router};
use iroh_gossip::{api::Event, api::GossipReceiver, net::Gossip, proto::TopicId};
use messaging::*;
use slint::{SharedString, Weak};
use std::sync::{Arc, Mutex};
use ticket::*;

use crate::app::{app_state::AppState, types, ui_handlers::update_messages};

const DEFAULT_RELAY_URL: &str = "https://relay.iroh.link";

pub async fn setup_networking(
    ticket: Option<Ticket>,
    username: String,
) -> Result<(
    iroh_gossip::api::GossipSender,
    GossipReceiver,
    Endpoint,
    Router,
    Ticket,
)> {
    let (topic, nodes) = match ticket {
        Some(Ticket { topic, nodes }) => {
            println!("> joining chat room for topic {topic}");
            (topic, nodes)
        }
        None => {
            let topic = TopicId::from_bytes(rand::random());
            println!("> opening chat room for topic {topic}");
            (topic, vec![])
        }
    };

    let endpoint = Endpoint::builder().discovery_n0().bind().await?;
    println!("> our node id: {}", endpoint.node_id());

    let gossip = Gossip::builder().spawn(endpoint.clone());
    let router = Router::builder(endpoint.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn();

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let ticket = {
        let mut node_addr = iroh::NodeAddr::new(endpoint.node_id());

        if let Some(_discovery) = endpoint.discovery() {
            let relay_url = DEFAULT_RELAY_URL.parse().expect("Invalid relay URL");
            node_addr = node_addr.with_relay_url(relay_url);
        }

        let nodes = vec![node_addr];
        Ticket { topic, nodes }
    };
    println!("> ticket to join us: {ticket}");

    let node_ids: Vec<NodeId> = nodes.iter().map(|p| p.node_id).collect();
    if !nodes.is_empty() {
        println!("> trying to connect to {} nodes...", nodes.len());
        for node in nodes.into_iter() {
            endpoint.add_node_addr(node)?;
        }

        let (sender, receiver) = gossip.subscribe_and_join(topic, node_ids).await?.split();
        println!("> connected!");
        println!("DEBUG: About to send AboutMe message");

        let message = Message::new(MessageBody::AboutMe {
            from: endpoint.node_id(),
            name: username,
        });
        println!("DEBUG: Created AboutMe message, about to broadcast");
        sender.broadcast(message.to_vec().into()).await?;
        println!("DEBUG: AboutMe message broadcast complete");

        Ok((sender, receiver, endpoint, router, ticket))
    } else {
        println!("> creating new room, subscribing to topic...");
        let subscription = gossip.subscribe(topic, vec![]).await?;
        let (sender, receiver) = subscription.split();
        println!("> connected!");
        println!("DEBUG: About to send AboutMe message");

        let message = Message::new(MessageBody::AboutMe {
            from: endpoint.node_id(),
            name: username,
        });
        println!("DEBUG: Created AboutMe message, about to broadcast");
        sender.broadcast(message.to_vec().into()).await?;
        println!("DEBUG: AboutMe message broadcast complete");

        Ok((sender, receiver, endpoint, router, ticket))
    }
}

pub async fn handle_messages(
    mut receiver: GossipReceiver,
    chat_handle: Weak<types::ChatWindow>,
    app_state: Arc<Mutex<AppState>>,
) -> Result<()> {
    while let Some(event) = receiver.try_next().await? {
        if let Event::Received(msg) = event {
            match Message::from_bytes(&msg.content)?.body {
                MessageBody::AboutMe { from, name } => {
                    {
                        let state = app_state.lock().unwrap();
                        let mut names = state.names.lock().unwrap();
                        names.insert(from, name.clone());
                    }

                    crate::app::ui_handlers::update_online_users(&chat_handle, &app_state);

                    println!("> {} is now known as {}", from.fmt_short(), name);
                }
                MessageBody::Message { from, text } => {
                    let (sender_name, is_own) = {
                        let state = app_state.lock().unwrap();
                        let names = state.names.lock().unwrap();
                        let sender_name = names
                            .get(&from)
                            .map_or_else(|| from.fmt_short(), String::to_string);
                        let is_own = state.current_node_id.map_or(false, |id| id == from);
                        (sender_name, is_own)
                    };

                    let new_message = types::ChatMessage {
                        username: SharedString::from(sender_name.clone()),
                        content: SharedString::from(text.clone()),
                        timestamp: SharedString::from(
                            chrono::Local::now().format("%H:%M").to_string(),
                        ),
                        is_own,
                        is_system: false,
                    };

                    {
                        let state = app_state.lock().unwrap();
                        let mut messages = state.messages.lock().unwrap();
                        messages.push(new_message);
                    }

                    update_messages(&chat_handle, &app_state);
                    println!(
                        "DEBUG: Message added to GUI - from {}: {}",
                        sender_name, text
                    );
                }
            }
        }
    }
    Ok(())
}

pub async fn send_message(message: String, app_state: Arc<Mutex<AppState>>) -> Result<()> {
    let (sender, node_id, username) = {
        let state = app_state.lock().unwrap();
        (
            state.sender.clone(),
            state.current_node_id,
            state.current_username.clone(),
        )
    };

    if let (Some(sender), Some(node_id)) = (sender, node_id) {
        let new_message = types::ChatMessage {
            username: SharedString::from(username),
            content: SharedString::from(message.clone()),
            timestamp: SharedString::from(chrono::Local::now().format("%H:%M").to_string()),
            is_own: true,
            is_system: false,
        };

        {
            let state = app_state.lock().unwrap();
            let mut messages = state.messages.lock().unwrap();
            messages.push(new_message);
        }

        let msg = Message::new(MessageBody::Message {
            from: node_id,
            text: message,
        });
        sender.broadcast(msg.to_vec().into()).await?;
    }

    Ok(())
}
