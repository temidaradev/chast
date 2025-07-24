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

            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(10))
                .validation_mode(gossipsub::ValidationMode::Strict)
                .message_id_fn(message_id_fn)
                .build()
                .map_err(io::Error::other)?;

            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )?;

            let mdns =
                mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;

            let relay_config = relay::Config {
                max_reservations: 128,
                max_reservations_per_peer: 4,
                reservation_duration: Duration::from_secs(60 * 60),
                max_circuits: 16,
                max_circuits_per_peer: 4,
                ..Default::default()
            };
            let relay = relay::Behaviour::new(key.public().to_peer_id(), relay_config);

            let autonat =
                autonat::Behaviour::new(key.public().to_peer_id(), autonat::Config::default());
            let dcutr = dcutr::Behaviour::new(key.public().to_peer_id());

            // Add Kademlia DHT for peer discovery
            let store = kad::store::MemoryStore::new(key.public().to_peer_id());
            let mut kademlia = kad::Behaviour::new(key.public().to_peer_id(), store);

            // Set Kademlia to server mode to help with routing
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

pub fn setup_kademlia_bootstrap(swarm: &mut P2PSwarm) -> Result<(), Box<dyn Error>> {
    // Add bootstrap nodes to Kademlia DHT
    let bootstrap_nodes = vec![
        ("/dnsaddr/bootstrap.libp2p.io", "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN"),
        ("/dnsaddr/bootstrap.libp2p.io", "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa"),
    ];

    for (addr_str, peer_id_str) in bootstrap_nodes {
        if let Ok(addr) = addr_str.parse::<Multiaddr>() {
            if let Ok(peer_id) = peer_id_str.parse() {
                swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
                swarm.dial(addr)?;
            }
        }
    }

    // Bootstrap the DHT
    if let Err(e) = swarm.behaviour_mut().kademlia.bootstrap() {
        println!("Kademlia bootstrap failed: {:?}", e);
    }

    Ok(())
}

pub fn connect_to_bootstrap_nodes(swarm: &mut P2PSwarm) -> Result<(), Box<dyn Error>> {
    let bootstrap_nodes = vec![
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
        "/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ",
    ];

    for addr_str in bootstrap_nodes {
        if let Ok(addr) = addr_str.parse::<Multiaddr>() {
            swarm.dial(addr.clone())?;
            println!("Connecting to bootstrap node: {}", addr);
        }
    }

    Ok(())
}

pub fn connect_to_relay_servers(swarm: &mut P2PSwarm) -> Result<(), Box<dyn Error>> {
    let relay_addresses = vec![
        "/ip4/147.75.83.83/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN",
        "/ip4/147.75.83.83/udp/4001/quic/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN",
    ];

    for addr_str in relay_addresses {
        if let Ok(addr) = addr_str.parse::<Multiaddr>() {
            swarm.dial(addr)?;
        }
    }

    Ok(())
}