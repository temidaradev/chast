use config::{AppConfig, init_logging};
use futures::stream::StreamExt;
use libp2p::Multiaddr;
use libp2p::swarm::SwarmEvent;
use messaging::{
    MessageHandler, handle_gossipsub_message, handle_mdns_discovered, handle_mdns_expired,
};
use network::{P2PBehaviourEvent, create_swarm, connect_to_relay_servers};
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

    // Listen on multiple addresses for better connectivity
    for addr in &config.listen_addresses {
        swarm.listen_on(addr.parse()?)?;
    }

    // Connect to bootstrap nodes for better peer discovery
    for bootstrap_addr in &config.bootstrap_nodes {
        if let Ok(addr) = bootstrap_addr.parse::<Multiaddr>() {
            swarm.dial(addr.clone())?;
            println!("Connecting to bootstrap node: {}", addr);
        }
    }

    // Connect to relay servers if enabled
    if config.enable_relay {
        if let Err(e) = connect_to_relay_servers(&mut swarm) {
            println!("Warning: Failed to connect to relay servers: {}", e);
        }
    }

    // Connect to specific remote peer if configured
    if let Some(remote_addr) = &config.remote_peer_address {
        let remote_multiaddr: Multiaddr = remote_addr.parse()?;
        swarm.dial(remote_multiaddr)?;
        println!("Dialing remote peer at {remote_addr}");
    }

    println!("P2P Node Started!");
    println!("- Local network discovery: Enabled (mDNS)");
    println!("- Relay support: {}", if config.enable_relay { "Enabled" } else { "Disabled" });
    println!("- Hole punching: {}", if config.enable_hole_punching { "Enabled" } else { "Disabled" });
    println!("\nEnter messages via STDIN and they will be sent to connected peers using Gossipsub");

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
                SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                    println!("Connected to peer: {} via {}", peer_id, endpoint.get_remote_address());
                }
                SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                    println!("Connection to {} closed: {:?}", peer_id, cause);
                }
                SwarmEvent::Behaviour(P2PBehaviourEvent::Autonat(event)) => {
                    println!("AutoNAT event: {:?}", event);
                }
                SwarmEvent::Behaviour(P2PBehaviourEvent::Dcutr(event)) => {
                    println!("DCUtR event: {:?}", event);
                }
                SwarmEvent::Behaviour(P2PBehaviourEvent::Relay(event)) => {
                    println!("Relay event: {:?}", event);
                }
                _ => {}
            }
        }
    }
}