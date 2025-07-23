use tracing_subscriber::EnvFilter;

pub fn init_logging() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
    Ok(())
}

pub struct AppConfig {
    pub topic_name: String,
    pub listen_addresses: Vec<String>,
    pub remote_peer_address: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            topic_name: "test-net".to_string(),
            listen_addresses: vec![
                "/ip4/0.0.0.0/udp/0/quic-v1".to_string(),
                "/ip4/0.0.0.0/tcp/0".to_string(),
                "/ip4/0.0.0.0/udp/0".to_string(),
            ],
            remote_peer_address: None,
        }
    }
}
