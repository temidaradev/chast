use anyhow::Result;
use client::{Net, RoomHistory};
use futures_lite::StreamExt;
use iroh::{NodeId, SecretKey};
use iroh_gossip::{api::Event as GossipEvent, api::GossipReceiver, proto::TopicId};
use messaging::{Event, MessageBody, Membership, new_event_id, now_timestamp};
use slint::{SharedString, Weak};
use std::sync::{Arc, Mutex};
use ticket::Ticket;

use crate::app::{
    app_state::AppState,
    types,
    ui_handlers::{update_members, update_messages, push_system},
};

const DEFAULT_RELAY_URL: &str = "https://relay.iroh.link";
const JOIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

pub enum Mode {
    Server,
    Peer { display_name: String },
}

pub struct Connected {
    pub sender: iroh_gossip::api::GossipSender,
    pub receiver: GossipReceiver,
    pub topic: TopicId,
    pub secret_key: SecretKey,
    pub ticket: Ticket,
    pub node_id: NodeId,
}

pub async fn join_gossip(net: &Net, ticket: Option<Ticket>, mode: Mode) -> Result<Connected> {
    let secret_key = net.secret_key.clone();
    let node_id = net.endpoint.node_id();

    let (topic, nodes) = match ticket {
        Some(ticket) => (ticket.topic, ticket.nodes),
        None => (TopicId::from_bytes(rand::random()), vec![]),
    };

    let shareable = {
        let relay_url = DEFAULT_RELAY_URL.parse().expect("invalid relay url");
        let node_addr = iroh::NodeAddr::new(node_id).with_relay_url(relay_url);
        Ticket {
            topic,
            nodes: vec![node_addr],
        }
    };

    let node_ids: Vec<NodeId> = nodes.iter().map(|node| node.node_id).collect();
    let bootstrapping = !nodes.is_empty();
    for node in nodes {
        net.endpoint.add_node_addr(node)?;
    }

    let (sender, receiver) = if bootstrapping {
        let joining = net.gossip.subscribe_and_join(topic, node_ids);
        match tokio::time::timeout(JOIN_TIMEOUT, joining).await {
            Ok(joined) => joined?.split(),
            Err(_) => anyhow::bail!(
                "nobody answered in this room. If it is a server room, the server may be offline or not hosting it yet"
            ),
        }
    } else {
        net.gossip.subscribe(topic, vec![]).await?.split()
    };

    let greeting = match mode {
        Mode::Server => MessageBody::Hello,
        Mode::Peer { display_name } => MessageBody::Introduce { display_name },
    };
    sender
        .broadcast(messaging::sign(&secret_key, greeting)?.into())
        .await?;

    Ok(Connected {
        sender,
        receiver,
        topic,
        secret_key,
        ticket: shareable,
        node_id,
    })
}

fn secret_of(app_state: &Arc<Mutex<AppState>>) -> Option<SecretKey> {
    app_state.lock().unwrap().secret_key.clone()
}

pub async fn request_sync(app_state: &Arc<Mutex<AppState>>) -> Result<()> {
    let (sender, since_seq) = {
        let state = app_state.lock().unwrap();
        (
            state.sender.clone(),
            state.history.lock().unwrap().last_seq,
        )
    };

    if let (Some(sender), Some(secret_key)) = (sender, secret_of(app_state)) {
        let bytes = messaging::sign(&secret_key, MessageBody::SyncRequest { since_seq })?;
        sender.broadcast(bytes.into()).await?;
    }

    Ok(())
}

fn persist(app_state: &Arc<Mutex<AppState>>) {
    let state = app_state.lock().unwrap();
    if let Some(topic) = state.current_topic {
        let history = state.history.lock().unwrap();
        if let Err(e) = history.save(&topic) {
            eprintln!("failed to save history: {}", e);
        }
    }
}

pub fn event_to_chat_message(event: &Event, my_user: &str) -> types::ChatMessage {
    types::ChatMessage {
        username: SharedString::from(event.sender_name.clone()),
        content: SharedString::from(event.text.clone()),
        timestamp: SharedString::from(format_timestamp(&event.timestamp)),
        is_own: event.sender_user == my_user,
        is_system: false,
    }
}

pub fn format_timestamp(raw: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(raw) {
        Ok(parsed) => parsed
            .with_timezone(&chrono::Local)
            .format("%d/%m/%Y %H:%M:%S")
            .to_string(),
        Err(_) => raw.to_string(),
    }
}

pub async fn handle_messages(
    mut receiver: GossipReceiver,
    chat_handle: Weak<types::ChatWindow>,
    app_state: Arc<Mutex<AppState>>,
) -> Result<()> {
    loop {
        {
            let state = app_state.lock().unwrap();
            if state.sender.is_none() {
                break;
            }
        }

        let event = match receiver.try_next().await {
            Ok(Some(event)) => event,
            Ok(None) => break,
            Err(e) => {
                let keep_going = app_state.lock().unwrap().sender.is_some();
                if keep_going {
                    eprintln!("error receiving message: {}", e);
                    continue;
                }
                break;
            }
        };

        let GossipEvent::Received(msg) = event else {
            continue;
        };

        let (from, body) = match messaging::verify(&msg.content) {
            Ok(verified) => verified,
            Err(e) => {
                eprintln!("ignored an unsigned message: {}", e);
                continue;
            }
        };

        let (server, node_id, my_user) = {
            let state = app_state.lock().unwrap();
            (
                state.server_node_id,
                state.current_node_id,
                state.my_user_id.clone(),
            )
        };

        let from_server = server == Some(from);

        match body {
            MessageBody::Presence {
                node_id: who,
                user_id,
                display_name,
                power_level,
                membership,
            } => {
                if !from_server {
                    continue;
                }

                {
                    let state = app_state.lock().unwrap();
                    let mut roster = state.roster.lock().unwrap();
                    let mut nodes = state.nodes.lock().unwrap();

                    if membership == Membership::Join {
                        roster.insert(
                            user_id.clone(),
                            crate::app::app_state::RosterEntry {
                                user_id: user_id.clone(),
                                display_name: display_name.clone(),
                                power_level,
                                membership,
                            },
                        );
                        nodes.insert(who, user_id.clone());
                    } else {
                        roster.remove(&user_id);
                        nodes.remove(&who);
                    }
                }

                update_members(&chat_handle, &app_state);
            }

            MessageBody::Rejected { to, reason } => {
                if !from_server || Some(to) != node_id {
                    continue;
                }
                push_system(&app_state, format!("the server refused: {}", reason));
                update_messages(&chat_handle, &app_state);
            }

            MessageBody::Message { id, text } => {
                let (sender_user, display) = {
                    let state = app_state.lock().unwrap();
                    let nodes = state.nodes.lock().unwrap();
                    let roster = state.roster.lock().unwrap();

                    match nodes.get(&from) {
                        Some(user_id) => {
                            let display = roster
                                .get(user_id)
                                .map(|entry| entry.display_name.clone())
                                .unwrap_or_else(|| user_id.clone());
                            (user_id.clone(), display)
                        }
                        None => (String::new(), from.fmt_short()),
                    }
                };

                let event = Event {
                    id,
                    seq: 0,
                    sender: from,
                    sender_user,
                    sender_name: display,
                    text,
                    timestamp: now_timestamp(),
                };

                let is_new = {
                    let state = app_state.lock().unwrap();
                    let mut history = state.history.lock().unwrap();
                    history.record(event.clone())
                };

                if is_new {
                    {
                        let state = app_state.lock().unwrap();
                        let mut messages = state.messages.lock().unwrap();
                        messages.push(event_to_chat_message(&event, &my_user));
                    }
                    persist(&app_state);
                    update_messages(&chat_handle, &app_state);
                }
            }

            MessageBody::SyncResponse {
                to,
                events,
                latest_seq,
            } => {
                if !from_server || Some(to) != node_id {
                    continue;
                }

                let mut added = 0;
                let mut highest = 0;

                {
                    let state = app_state.lock().unwrap();
                    let mut history = state.history.lock().unwrap();
                    let mut messages = state.messages.lock().unwrap();

                    for event in events {
                        highest = highest.max(event.seq);
                        if history.record(event.clone()) {
                            messages.push(event_to_chat_message(&event, &my_user));
                            added += 1;
                        }
                    }
                    history.advance(highest);
                }

                if added > 0 {
                    push_system(
                        &app_state,
                        format!("restored {} message(s) from the server", added),
                    );
                }

                persist(&app_state);
                update_messages(&chat_handle, &app_state);

                if highest > 0 && highest < latest_seq {
                    let app_state = app_state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = request_sync(&app_state).await {
                            eprintln!("failed to continue sync: {}", e);
                        }
                    });
                }
            }

            MessageBody::Introduce { display_name } => {
                if server.is_some() {
                    continue;
                }

                let is_new = {
                    let state = app_state.lock().unwrap();
                    let mut roster = state.roster.lock().unwrap();
                    let mut nodes = state.nodes.lock().unwrap();

                    let user_id = from.to_string();
                    nodes.insert(from, user_id.clone());
                    roster
                        .insert(
                            user_id.clone(),
                            crate::app::app_state::RosterEntry {
                                user_id,
                                display_name: display_name.clone(),
                                power_level: 0,
                                membership: Membership::Join,
                            },
                        )
                        .is_none()
                };

                update_members(&chat_handle, &app_state);

                if is_new {
                    push_system(&app_state, format!("{} joined", display_name));
                    update_messages(&chat_handle, &app_state);

                    let (sender, my_name) = {
                        let state = app_state.lock().unwrap();
                        (state.sender.clone(), state.display_name.clone())
                    };

                    if let (Some(sender), Some(secret_key)) = (sender, secret_of(&app_state)) {
                        let bytes = messaging::sign(
                            &secret_key,
                            MessageBody::Introduce {
                                display_name: my_name,
                            },
                        )?;
                        if let Err(e) = sender.broadcast(bytes.into()).await {
                            eprintln!("failed to introduce ourselves: {}", e);
                        }
                    }
                }
            }

            MessageBody::Goodbye => {
                if server.is_some() {
                    continue;
                }

                let left = {
                    let state = app_state.lock().unwrap();
                    let mut roster = state.roster.lock().unwrap();
                    let mut nodes = state.nodes.lock().unwrap();

                    match nodes.remove(&from) {
                        Some(user_id) => roster.remove(&user_id).map(|e| e.display_name),
                        None => None,
                    }
                };

                if let Some(name) = left {
                    push_system(&app_state, format!("{} left", name));
                    update_messages(&chat_handle, &app_state);
                }
                update_members(&chat_handle, &app_state);
            }

            MessageBody::Hello | MessageBody::SyncRequest { .. } => {}
        }
    }

    Ok(())
}

pub async fn send_message(text: String, app_state: Arc<Mutex<AppState>>) -> Result<()> {
    let (sender, my_user, display_name) = {
        let state = app_state.lock().unwrap();
        (
            state.sender.clone(),
            state.my_user_id.clone(),
            state.display_name.clone(),
        )
    };

    let Some(secret_key) = secret_of(&app_state) else {
        return Ok(());
    };

    if let Some(sender) = sender {
        let event = Event {
            id: new_event_id(),
            seq: 0,
            sender: secret_key.public(),
            sender_user: my_user.clone(),
            sender_name: display_name,
            text: text.clone(),
            timestamp: now_timestamp(),
        };

        {
            let state = app_state.lock().unwrap();
            state.history.lock().unwrap().record(event.clone());
            let mut messages = state.messages.lock().unwrap();
            messages.push(event_to_chat_message(&event, &my_user));
        }

        persist(&app_state);

        let bytes = messaging::sign(
            &secret_key,
            MessageBody::Message {
                id: event.id,
                text,
            },
        )?;
        sender.broadcast(bytes.into()).await?;
    }

    Ok(())
}

pub async fn send_goodbye(app_state: Arc<Mutex<AppState>>) -> Result<()> {
    let sender = app_state.lock().unwrap().sender.clone();

    if let (Some(sender), Some(secret_key)) = (sender, secret_of(&app_state)) {
        let bytes = messaging::sign(&secret_key, MessageBody::Goodbye)?;
        sender.broadcast(bytes.into()).await?;
    }

    Ok(())
}

pub async fn cleanup_network_resources(app_state: Arc<Mutex<AppState>>) -> Result<()> {
    persist(&app_state);

    {
        let mut state = app_state.lock().unwrap();
        state.sender = None;
        state.current_node_id = None;
        state.current_topic = None;
        state.server_node_id = None;
        state.roster.lock().unwrap().clear();
        state.messages.lock().unwrap().clear();
        *state.history.lock().unwrap() = RoomHistory::default();
    }

    Ok(())
}
