use anyhow::Result;
use client::load_or_create_secret_key;
use futures_lite::StreamExt;
use iroh::{Endpoint, NodeId, protocol::Router};
use iroh_gossip::{api::Event as GossipEvent, net::Gossip, proto::TopicId};
use messaging::MessageBody;
use std::str::FromStr;
use ticket::Ticket;

const RELAY: &str = "https://relay.iroh.link";

#[tokio::main]
async fn main() -> Result<()> {
    let name = std::env::args().nth(1).unwrap_or_else(|| "peer".to_string());
    let ticket = std::env::args().nth(2);
    let text = std::env::args().nth(3);

    let secret_key = load_or_create_secret_key()?;
    let (topic, nodes) = match ticket {
        Some(raw) => {
            let ticket = Ticket::from_str(&raw)?;
            (ticket.topic, ticket.nodes)
        }
        None => (TopicId::from_bytes(rand::random()), vec![]),
    };

    let endpoint = Endpoint::builder()
        .secret_key(secret_key.clone())
        .discovery_n0()
        .bind()
        .await?;
    let gossip = Gossip::builder().spawn(endpoint.clone());
    let _router = Router::builder(endpoint.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn();

    tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;

    let share = Ticket {
        topic,
        nodes: vec![iroh::NodeAddr::new(endpoint.node_id()).with_relay_url(RELAY.parse()?)],
    };
    println!("TICKET {}", share);

    let node_ids: Vec<NodeId> = nodes.iter().map(|n| n.node_id).collect();
    let bootstrapping = !nodes.is_empty();
    for node in nodes {
        endpoint.add_node_addr(node)?;
    }

    let (sender, mut receiver) = if bootstrapping {
        gossip.subscribe_and_join(topic, node_ids).await?.split()
    } else {
        gossip.subscribe(topic, vec![]).await?.split()
    };

    sender
        .broadcast(
            messaging::sign(
                &secret_key,
                MessageBody::Introduce {
                    display_name: name.clone(),
                },
            )?
            .into(),
        )
        .await?;
    println!("{} introduced itself", name);

    if let Some(text) = text {
        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
        sender
            .broadcast(
                messaging::sign(
                    &secret_key,
                    MessageBody::Message {
                        id: messaging::new_event_id(),
                        text: text.clone(),
                    },
                )?
                .into(),
            )
            .await?;
        println!("{} sent: {}", name, text);
    }

    let mut seen = std::collections::HashSet::new();
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(20);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let Ok(Ok(Some(event))) = tokio::time::timeout(remaining, receiver.try_next()).await else {
            break;
        };
        let GossipEvent::Received(msg) = event else {
            continue;
        };
        let Ok((from, body)) = messaging::verify(&msg.content) else {
            continue;
        };
        match body {
            MessageBody::Introduce { display_name } => {
                if !seen.insert(from) {
                    continue;
                }
                println!("SAW peer {} ({})", display_name, from.fmt_short());
                sender
                    .broadcast(
                        messaging::sign(
                            &secret_key,
                            MessageBody::Introduce {
                                display_name: name.clone(),
                            },
                        )?
                        .into(),
                    )
                    .await?;
            }
            MessageBody::Message { text, .. } => {
                println!("SAW message from {}: {}", from.fmt_short(), text);
            }
            _ => {}
        }
    }

    Ok(())
}
