use config::{AppConfig, init_logging};
use futures::stream::StreamExt;
use libp2p::swarm::SwarmEvent;
use messaging::{
    MessageHandler, handle_gossipsub_message, handle_mdns_discovered, handle_mdns_expired,
};
use network::{P2PBehaviourEvent, create_swarm};
use std::error::Error;
use tokio::{io, io::AsyncBufReadExt, select};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_logging()?;

    let config = AppConfig::default();
    let mut swarm = create_swarm()?;
    let message_handler = MessageHandler::new(&config.topic_name);

    message_handler.subscribe(&mut swarm)?;

    let mut stdin = io::BufReader::new(io::stdin()).lines();

    for addr in &config.listen_addresses {
        swarm.listen_on(addr.parse()?)?;
    }

    println!("Enter messages via STDIN and they will be sent to connected peers using Gossipsub");
    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                if let Err(e) = message_handler.publish_message(&mut swarm, &line) {
                    println!("Error publishing message: {e:?}");
                }
            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(P2PBehaviourEvent::Mdns(
                    libp2p::mdns::Event::Discovered(list)
                )) => {
                    handle_mdns_discovered(&mut swarm, list);
                },
                SwarmEvent::Behaviour(P2PBehaviourEvent::Mdns(
                    libp2p::mdns::Event::Expired(list)
                )) => {
                    handle_mdns_expired(&mut swarm, list);
                },
                SwarmEvent::Behaviour(P2PBehaviourEvent::Gossipsub(
                    libp2p::gossipsub::Event::Message {
                        propagation_source: peer_id,
                        message_id: id,
                        message,
                    }
                )) => {
                    handle_gossipsub_message(peer_id, id, message);
                },
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Local node is listening on {address}");
                }
                _ => {}
            }
        }
    }
}
