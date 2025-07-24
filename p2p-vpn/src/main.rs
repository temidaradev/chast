use config::{AppConfig, init_logging};
use futures::stream::StreamExt;
use libp2p::swarm::SwarmEvent;
use messaging::{
    MessageHandler, handle_gossipsub_message, handle_mdns_discovered, handle_mdns_expired,
};
use network::{P2PBehaviourEvent, create_swarm, initialize_auto_discovery, perform_peer_discovery};
use std::error::Error;
use tokio::{io, io::AsyncBufReadExt, select, time};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_logging()?;

    let config = AppConfig::default();
    let mut swarm = create_swarm()?;
    let message_handler = MessageHandler::new(&config.topic_name);

    message_handler.subscribe(&mut swarm)?;

    // Listen on all available interfaces
    let listen_addresses = vec![
        "/ip4/0.0.0.0/tcp/0",
        "/ip4/0.0.0.0/udp/0/quic-v1",
        "/ip6/::/tcp/0",
        "/ip6/::/udp/0/quic-v1",
    ];

    for addr in listen_addresses {
        if let Ok(multiaddr) = addr.parse() {
            let _ = swarm.listen_on(multiaddr);
        }
    }

    // Initialize automatic peer discovery
    initialize_auto_discovery(&mut swarm)?;

    let mut stdin = io::BufReader::new(io::stdin()).lines();

    // Timers for continuous discovery
    let mut discovery_timer = time::interval(time::Duration::from_secs(30));
    let mut status_timer = time::interval(time::Duration::from_secs(10));

    println!("🚀 P2P Auto-Discovery Node Started!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🔍 Automatic Peer Discovery Features:");
    println!("   • mDNS: Finds peers on local network");
    println!("   • DHT: Discovers peers globally via Kademlia");
    println!("   • Bootstrap: Connects to public libp2p nodes");
    println!("   • Relay: NAT traversal for hard-to-reach peers");
    println!("   • AutoNAT: Detects network connectivity status");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("\n💬 Type messages to broadcast to all connected peers:");
    println!("📊 Connection status will be shown every 10 seconds\n");

    loop {
        select! {
            // Handle user input
            Ok(Some(line)) = stdin.next_line() => {
                if line.trim().is_empty() {
                    continue;
                }

                let connected_peers = swarm.connected_peers().count();
                if connected_peers == 0 {
                    println!("⚠️  No peers connected yet. Message queued for when peers connect.");
                }

                if let Err(e) = message_handler.publish_message(&mut swarm, &line) {
                    println!("❌ Error publishing message: {e:?}");
                }
            }

            // Periodic peer discovery
            _ = discovery_timer.tick() => {
                perform_peer_discovery(&mut swarm);
                println!("🔍 Searching for more peers...");
            }

            // Status updates
            _ = status_timer.tick() => {
                let connected_peers = swarm.connected_peers().count();
                let listening_addrs = swarm.listeners().count();

                println!("📊 Status: {} peers connected, listening on {} addresses",
                    connected_peers, listening_addrs);

                if connected_peers == 0 {
                    println!("   🔍 Still discovering peers...");
                }
            }

            // Handle network events
            event = swarm.select_next_some() => match event {
                // mDNS discovered local peers
                SwarmEvent::Behaviour(P2PBehaviourEvent::Mdns(
                    libp2p::mdns::Event::Discovered(list)
                )) => {
                    for (peer_id, _addr) in &list {
                        println!("🏠 Found local peer: {}", peer_id);
                    }
                    handle_mdns_discovered(&mut swarm, list);
                },

                // mDNS peer expired
                SwarmEvent::Behaviour(P2PBehaviourEvent::Mdns(
                    libp2p::mdns::Event::Expired(list)
                )) => {
                    for (peer_id, _addr) in &list {
                        println!("🏠 Lost local peer: {}", peer_id);
                    }
                    handle_mdns_expired(&mut swarm, list);
                },

                // Received message from another peer
                SwarmEvent::Behaviour(P2PBehaviourEvent::Gossipsub(
                    libp2p::gossipsub::Event::Message {
                        propagation_source: peer_id,
                        message_id: id,
                        message,
                    }
                )) => {
                    handle_gossipsub_message(peer_id, id, message);
                },

                // DHT events - peer discovery
                SwarmEvent::Behaviour(P2PBehaviourEvent::Kademlia(event)) => {
                    match event {
                        libp2p::kad::Event::OutboundQueryProgressed { result, .. } => {
                            match result {
                                libp2p::kad::QueryResult::GetClosestPeers(Ok(ok)) => {
                                    for peer in ok.peers {
                                        println!("🌐 DHT discovered peer: {}", peer.peer_id);
                                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer.peer_id);
                                    }
                                }
                                libp2p::kad::QueryResult::Bootstrap(Ok(_)) => {
                                    println!("✅ DHT bootstrap successful");
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                },

                // Connection events
                SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                    println!("✅ Connected to: {} via {}", peer_id, endpoint.get_remote_address());
                    swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                }

                SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                    println!("❌ Disconnected from {}: {:?}", peer_id, cause);
                }

                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("🎧 Listening on: {}", address);
                }

                SwarmEvent::Behaviour(P2PBehaviourEvent::Autonat(event)) => {
                    match event {
                        libp2p::autonat::Event::StatusChanged { old, new } => {
                            println!("🔄 NAT status changed: {:?} -> {:?}", old, new);
                        }
                        _ => {}
                    }
                }

                _ => {}
            }
        }
    }
}