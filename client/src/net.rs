use anyhow::Result;
use iroh::{Endpoint, SecretKey, protocol::Router};
use iroh_gossip::net::Gossip;

use crate::identity::load_or_create_secret_key;

pub struct Net {
    pub endpoint: Endpoint,
    pub gossip: Gossip,
    pub router: Router,
    pub secret_key: SecretKey,
}

impl Net {
    pub async fn start() -> Result<Self> {
        let secret_key = load_or_create_secret_key()?;

        let endpoint = Endpoint::builder()
            .secret_key(secret_key.clone())
            .discovery_n0()
            .bind()
            .await?;

        let gossip = Gossip::builder().spawn(endpoint.clone());
        let router = Router::builder(endpoint.clone())
            .accept(iroh_gossip::ALPN, gossip.clone())
            .spawn();

        tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;

        Ok(Self {
            endpoint,
            gossip,
            router,
            secret_key,
        })
    }

    pub async fn shutdown(self) {
        self.router.shutdown().await.ok();
        self.endpoint.close().await;
    }
}
