use libp2p::{gossipsub, mdns};
use network::{P2PBehaviour, P2PSwarm};
use std::error::Error;

pub struct MessageHandler {
    topic: gossipsub::IdentTopic,
}

impl MessageHandler {
    pub fn new(topic_name: &str) -> Self {
        Self {
            topic: gossipsub::IdentTopic::new(topic_name),
        }
    }

    pub fn subscribe(&self, swarm: &mut P2PSwarm) -> Result<(), Box<dyn Error>> {
        swarm.behaviour_mut().gossipsub.subscribe(&self.topic)?;
        Ok(())
    }

    pub fn publish_message(
        &self,
        swarm: &mut P2PSwarm,
        message: &str,
    ) -> Result<(), Box<dyn Error>> {
        if let Err(e) = swarm
            .behaviour_mut()
            .gossipsub
            .publish(self.topic.clone(), message.as_bytes())
        {
            return Err(format!("Publish error: {e:?}").into());
        }
        Ok(())
    }
}

pub fn handle_mdns_discovered(
    swarm: &mut P2PSwarm,
    list: Vec<(libp2p::PeerId, libp2p::Multiaddr)>,
) {
    for (peer_id, _multiaddr) in list {
        println!("mDNS discovered a new peer: {peer_id}");
        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
    }
}

pub fn handle_mdns_expired(swarm: &mut P2PSwarm, list: Vec<(libp2p::PeerId, libp2p::Multiaddr)>) {
    for (peer_id, _multiaddr) in list {
        println!("mDNS discover peer has expired: {peer_id}");
        swarm
            .behaviour_mut()
            .gossipsub
            .remove_explicit_peer(&peer_id);
    }
}

pub fn handle_gossipsub_message(
    peer_id: libp2p::PeerId,
    message_id: gossipsub::MessageId,
    message: gossipsub::Message,
) {
    println!(
        "Got message: '{}' with id: {message_id} from peer: {peer_id}",
        String::from_utf8_lossy(&message.data),
    );
}
