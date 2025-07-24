use tracing_subscriber::EnvFilter;

pub fn init_logging() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
    Ok(())
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub topic_name: String,
    pub listen_addresses: Vec<String>,
    pub remote_peer_address: Option<String>,
    pub bootstrap_nodes: Vec<String>,
    pub enable_relay: bool,
    pub enable_hole_punching: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            topic_name: "p2p-chat".to_string(),
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/0".to_string(),
                "/ip4/0.0.0.0/udp/0/quic-v1".to_string(),
            ],
            remote_peer_address: None,
            bootstrap_nodes: vec![
                // Add some bootstrap nodes here
                "/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ".to_string(),
            ],
            enable_relay: true,
            enable_hole_punching: true,
        }
    }
}
