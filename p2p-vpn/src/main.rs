use config::{AppConfig, init_logging};
use futures::stream::StreamExt;
use libp2p::swarm::SwarmEvent;
use messaging::{
    MessageHandler, handle_gossipsub_message, handle_mdns_discovered, handle_mdns_expired,
};
use network::{
    P2PBehaviourEvent, create_swarm, connect_to_bootstrap_nodes,
    connect_to_relay_servers, setup_kademlia_bootstrap
};
use std::error::Error;
use tokio::{io, io::AsyncBufReadExt, select, time};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_logging()?;

    let config = AppConfig::default();
    let mut swarm = create_swarm()?;
    let message_handler = MessageHandler::new(&config.topic_name);

    message_handler.subscribe(&mut swarm)?;

    let mut stdin = io::BufReader::new(io::stdin()).lines();

    // Listen on multiple addresses
    for addr in &config.listen_addresses {
        swarm.listen_on(addr.parse()?)?;
    }

    // Automatic peer discovery setup
    println!("🚀 Starting P2P node with automatic peer discovery...");

    // Connect to bootstrap nodes for initial peer discovery
    if let Err(e) = connect_to_bootstrap_nodes(&mut swarm) {
        println!("Warning: Failed to connect to bootstrap nodes: {}", e);
    }

    // Connect to relay servers for NAT traversal
    if let Err(e) = connect_to_relay_servers(&mut swarm) {
        println!("Warning: Failed to connect to relay servers: {}", e);
    }

    // Setup Kademlia DHT for peer discovery
    if let Err(e) = setup_kademlia_bootstrap(&mut swarm) {
        println!("Warning: Failed to setup Kademlia bootstrap: {}", e);
    }

    // Periodic peer discovery
    let mut discovery_interval = time::interval(time::Duration::from_secs(30));

    println!("✅ Node started! Automatic peer discovery enabled:");
    println!("   • mDNS: Local network discovery");
    println!("   • DHT: Global peer discovery");
    println!("   • Relay: NAT traversal");
    println!("   • Bootstrap: Initial peer connections");
    println!("\nEnter messages to broadcast to all connected peers:");

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                if let Err(e) = message_handler.publish_message(&mut swarm, &line) {
                    println!("Error publishing message: {e:?}");
                }
            }
            _ = discovery_interval.tick() => {
                // Periodic peer discovery
                let connected_peers = swarm.connected_peers().count();
                println!("📊 Connected peers: {}", connected_peers);

                if connected_peers < 3 {
                    // Try to find more peers
                    let local_peer_id = swarm.local_peer_id().to_bytes();
                    swarm.behaviour_mut().kademlia.get_closest_peers(local_peer_id);
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
                SwarmEvent::Behaviour(P2PBehaviourEvent::Kademlia(event)) => {
                    match event {
                        libp2p::kad::Event::OutboundQueryProgressed { result, .. } => {
                            match result {
                                libp2p::kad::QueryResult::GetClosestPeers(Ok(ok)) => {
                                    for peer in ok.peers {
                                        println!("🔍 Discovered peer via DHT: {}", peer.peer_id);
                                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer.peer_id);
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                },
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("🎧 Listening on: {address}");
                }
                SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                    println!("✅ Connected to: {} via {}", peer_id, endpoint.get_remote_address());
                    swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                }
                SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                    println!("❌ Disconnected from {}: {:?}", peer_id, cause);
                }
                _ => {}
            }
        }
    }
}