use libp2p::{
    SwarmBuilder, autonat, dcutr, gossipsub, identity, mdns, noise, relay, kad,
    swarm::{NetworkBehaviour, Swarm},
    tcp, yamux, Multiaddr,
};
use std::{
    collections::hash_map::DefaultHasher,
    error::Error,
    hash::{Hash, Hasher},
    time::Duration,
};
use tokio::io;

#[derive(NetworkBehaviour)]
pub struct P2PBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub relay: relay::Behaviour,
    pub autonat: autonat::Behaviour,
    pub dcutr: dcutr::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
}

pub type P2PSwarm = Swarm<P2PBehaviour>;

pub fn create_swarm() -> Result<P2PSwarm, Box<dyn Error>> {
    let swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_relay_client(noise::Config::new, yamux::Config::default)?
        .with_behaviour(|key, _relay_behaviour| {
            let message_id_fn = |message: &gossipsub::Message| {
                let mut s = DefaultHasher::new();
                message.data.hash(&mut s);
                gossipsub::MessageId::from(s.finish().to_string())
            };

            // Enhanced Gossipsub config for better auto-discovery
            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(5)) // More frequent heartbeats
                .validation_mode(gossipsub::ValidationMode::Strict)
                .message_id_fn(message_id_fn)
                .mesh_n_high(12) // Higher mesh connectivity
                .mesh_n_low(4)   // Minimum mesh connections
                .gossip_lazy(6)  // More gossip propagation
                .fanout_ttl(Duration::from_secs(60))
                .build()
                .map_err(io::Error::other)?;

            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )?;

            // Enhanced mDNS for local network discovery
            let mdns_config = mdns::Config {
                ttl: Duration::from_secs(60),
                query_interval: Duration::from_secs(5), // Query more frequently
                enable_ipv6: true,
            };
            let mdns = mdns::tokio::Behaviour::new(mdns_config, key.public().to_peer_id())?;

            let relay_config = relay::Config {
                max_reservations: 128,
                max_reservations_per_peer: 4,
                reservation_duration: Duration::from_secs(60 * 60),
                max_circuits: 16,
                max_circuits_per_peer: 4,
                ..Default::default()
            };
            let relay = relay::Behaviour::new(key.public().to_peer_id(), relay_config);

            let autonat = autonat::Behaviour::new(key.public().to_peer_id(), autonat::Config::default());
            let dcutr = dcutr::Behaviour::new(key.public().to_peer_id());

            // Enhanced Kademlia for better peer discovery
            let store = kad::store::MemoryStore::new(key.public().to_peer_id());
            let mut kad_config = kad::Config::default();
            kad_config.set_query_timeout(Duration::from_secs(60));
            kad_config.set_replication_factor(3.try_into().unwrap());

            let mut kademlia = kad::Behaviour::with_config(key.public().to_peer_id(), store, kad_config);
            kademlia.set_mode(Some(kad::Mode::Server));

            Ok(P2PBehaviour {
                gossipsub,
                mdns,
                relay,
                autonat,
                dcutr,
                kademlia,
            })
        })?
        .build();

    Ok(swarm)
}

// Auto-discovery initialization - this makes machines find each other automatically
pub fn initialize_auto_discovery(swarm: &mut P2PSwarm) -> Result<(), Box<dyn Error>> {
    println!("🔍 Initializing automatic peer discovery...");

    // Step 1: Connect to public bootstrap nodes for global discovery
    let bootstrap_nodes = vec![
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
        "/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ",
    ];

    for addr_str in bootstrap_nodes {
        if let Ok(addr) = addr_str.parse::<Multiaddr>() {
            swarm.dial(addr.clone())?;

            // Also add to Kademlia routing table
            if let Some(peer_id) = addr.iter().find_map(|protocol| {
                if let libp2p::multiaddr::Protocol::P2p(peer_id) = protocol {
                    Some(peer_id)
                } else {
                    None
                }
            }) {
                swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
            }
        }
    }

    // Step 2: Connect to relay servers for NAT traversal
    let relay_servers = vec![
        "/ip4/147.75.83.83/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN",
        "/ip4/147.75.83.83/udp/4001/quic/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN",
    ];

    for addr_str in relay_servers {
        if let Ok(addr) = addr_str.parse::<Multiaddr>() {
            swarm.dial(addr)?;
        }
    }

    // Step 3: Bootstrap Kademlia DHT
    if let Err(e) = swarm.behaviour_mut().kademlia.bootstrap() {
        println!("DHT bootstrap warning: {:?}", e);
    }

    println!("✅ Auto-discovery initialized!");
    Ok(())
}

// Continuous peer discovery - keeps finding new peers
pub fn perform_peer_discovery(swarm: &mut P2PSwarm) {
    let local_peer_id = *swarm.local_peer_id();

    // Search for peers similar to our ID
    swarm.behaviour_mut().kademlia.get_closest_peers(local_peer_id.to_bytes());

    // Also search for random peers to expand network
    let random_peer_id = identity::Keypair::generate_ed25519().public().to_peer_id();
    swarm.behaviour_mut().kademlia.get_closest_peers(random_peer_id.to_bytes());
}