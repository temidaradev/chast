use libp2p::{
    SwarmBuilder, autonat, dcutr, gossipsub, identity, mdns, noise, relay,
    swarm::{NetworkBehaviour, Swarm},
    tcp, yamux,
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
        .with_behaviour(|key| {
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

            let relay = relay::Behaviour::new(key.public().to_peer_id(), relay::Config::default());
            let autonat =
                autonat::Behaviour::new(key.public().to_peer_id(), autonat::Config::default());
            let dcutr = dcutr::Behaviour::new(key.public().to_peer_id());

            Ok(P2PBehaviour {
                gossipsub,
                mdns,
                relay,
                autonat,
                dcutr,
            })
        })?
        .build();

    Ok(swarm)
}
