use anyhow::{Context, Result};
use client::load_or_create_secret_key;
use futures_lite::StreamExt;
use iroh::{Endpoint, protocol::Router};
use iroh_gossip::{api::Event as GossipEvent, net::Gossip};
use messaging::MessageBody;
use std::str::FromStr;
use ticket::Ticket;

#[tokio::main]
async fn main() -> Result<()> {
    let ticket = std::env::args()
        .nth(1)
        .context("usage: intruder <leaked-ticket>")?;
    let ticket = Ticket::from_str(&ticket)?;

    let secret_key = load_or_create_secret_key()?;
    let endpoint = Endpoint::builder()
        .secret_key(secret_key.clone())
        .discovery_n0()
        .bind()
        .await?;
    let node_id = endpoint.node_id();
    println!("intruder node {}", node_id.fmt_short());

    let gossip = Gossip::builder().spawn(endpoint.clone());
    let _router = Router::builder(endpoint.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn();

    let node_ids: Vec<iroh::NodeId> = ticket.nodes.iter().map(|n| n.node_id).collect();
    for node in ticket.nodes {
        endpoint.add_node_addr(node)?;
    }

    let (sender, mut receiver) = gossip
        .subscribe_and_join(ticket.topic, node_ids)
        .await?
        .split();
    println!("intruder joined the gossip topic with the leaked ticket");

    sender
        .broadcast(messaging::sign(&secret_key, MessageBody::Hello)?.into())
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    sender
        .broadcast(
            messaging::sign(
                &secret_key,
                MessageBody::Message {
                    id: messaging::new_event_id(),
                    text: "INTRUDER POST".to_string(),
                },
            )?
            .into(),
        )
        .await?;
    sender
        .broadcast(
            messaging::sign(&secret_key, MessageBody::SyncRequest { since_seq: 0 })?.into(),
        )
        .await?;
    println!("intruder sent a message and a sync request");

    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            println!("RESULT no history was ever delivered to the intruder");
            break;
        }

        let Ok(Ok(Some(event))) = tokio::time::timeout(remaining, receiver.try_next()).await else {
            println!("RESULT no history was ever delivered to the intruder");
            break;
        };

        let GossipEvent::Received(msg) = event else {
            continue;
        };
        let Ok((_, body)) = messaging::verify(&msg.content) else {
            continue;
        };

        match body {
            MessageBody::SyncResponse { to, events, .. } if to == node_id => {
                println!("RESULT LEAK, received {} events", events.len());
                break;
            }
            MessageBody::Rejected { to, reason } if to == node_id => {
                println!("RESULT refused by the server: {}", reason);
                break;
            }
            _ => {}
        }
    }

    Ok(())
}
