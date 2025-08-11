use anyhow::Result;
use clap::Parser;
use futures_lite::StreamExt;
use iroh::{Endpoint, Watcher, protocol::Router};
use iroh_gossip::{api::Event, api::GossipReceiver, net::Gossip, proto::TopicId};
use messaging::*;
use std::collections::HashMap;
use std::str::FromStr;
use ticket::*;

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long)]
    name: Option<String>,
    #[clap(short, long, default_value = "0")]
    bind_port: u16,
    #[clap(subcommand)]
    command: Command,
}

#[derive(Parser, Debug)]
enum Command {
    Open,
    Join { ticket: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let (topic, nodes) = match &args.command {
        Command::Open => {
            let topic = TopicId::from_bytes(rand::random());
            println!("> opening chat room for topic {topic}");
            (topic, vec![])
        }
        Command::Join { ticket } => {
            let Ticket { topic, nodes } = Ticket::from_str(ticket)?;
            println!("> joining chat room for topic {topic}");
            (topic, nodes)
        }
    };

    let endpoint = Endpoint::builder().discovery_n0().bind().await?;

    println!("> our node id: {}", endpoint.node_id());
    let gossip = Gossip::builder().spawn(endpoint.clone());

    let router = Router::builder(endpoint.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn();

    let ticket = {
        let me = endpoint
            .node_addr()
            .get()
            .expect("REASON")
            .ok_or_else(|| anyhow::anyhow!("No node address available"))?;
        let nodes = vec![me];
        Ticket { topic, nodes }
    };
    println!("> ticket to join us: {ticket}");

    let node_ids = nodes.iter().map(|p| p.node_id).collect();
    if nodes.is_empty() {
        println!("> waiting for nodes to join...");
    } else {
        println!("> trying to connect to {} nodes...", nodes.len());
        for node in nodes.into_iter() {
            endpoint.add_node_addr(node)?;
        }
    };
    let (mut sender, receiver) = gossip.subscribe_and_join(topic, node_ids).await?.split();
    println!("> connected!");

    if let Some(name) = args.name {
        let message = Message::new(MessageBody::AboutMe {
            from: endpoint.node_id(),
            name,
        });
        sender.broadcast(message.to_vec().into()).await?;
    }

    tokio::spawn(subscribe_loop(receiver));

    let (line_tx, mut line_rx) = tokio::sync::mpsc::channel(1);
    std::thread::spawn(move || input_loop(line_tx));

    println!("> type a message and hit enter to broadcast...");
    while let Some(text) = line_rx.recv().await {
        let message = Message::new(MessageBody::Message {
            from: endpoint.node_id(),
            text: text.clone(),
        });
        sender.broadcast(message.to_vec().into()).await?;
        println!("> sent: {text}");
    }

    router.shutdown().await?;

    Ok(())
}

async fn subscribe_loop(mut receiver: GossipReceiver) -> Result<()> {
    let mut names = HashMap::new();
    while let Some(event) = receiver.try_next().await? {
        if let Event::Received(msg) = event {
            match Message::from_bytes(&msg.content)?.body {
                MessageBody::AboutMe { from, name } => {
                    names.insert(from, name.clone());
                    println!("> {} is now known as {}", from.fmt_short(), name);
                }
                MessageBody::Message { from, text } => {
                    let name = names
                        .get(&from)
                        .map_or_else(|| from.fmt_short(), String::to_string);
                    println!("{name}: {text}");
                }
            }
        }
    }
    Ok(())
}

fn input_loop(line_tx: tokio::sync::mpsc::Sender<String>) -> Result<()> {
    let mut buffer = String::new();
    let stdin = std::io::stdin();
    loop {
        stdin.read_line(&mut buffer)?;
        line_tx.blocking_send(buffer.clone())?;
        buffer.clear();
    }
}
