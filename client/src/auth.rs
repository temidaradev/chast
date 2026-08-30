use anyhow::{Context, Result};
use iroh::{Endpoint, NodeAddr, NodeId, SecretKey};
use messaging::rpc::{AUTH_ALPN, Request, Response, read_frame, write_frame};

const DEFAULT_RELAY_URL: &str = "https://relay.iroh.link";

pub fn server_addr(node_id: NodeId) -> NodeAddr {
    let relay_url = DEFAULT_RELAY_URL.parse().expect("invalid relay url");
    NodeAddr::new(node_id).with_relay_url(relay_url)
}

pub struct AuthClient {
    endpoint: Endpoint,
    server: NodeId,
}

impl AuthClient {
    pub async fn connect(secret_key: SecretKey, server: NodeId) -> Result<Self> {
        let endpoint = Endpoint::builder()
            .secret_key(secret_key)
            .discovery_n0()
            .bind()
            .await?;

        Ok(Self { endpoint, server })
    }

    pub fn from_endpoint(endpoint: Endpoint, server: NodeId) -> Self {
        Self { endpoint, server }
    }

    pub async fn call(&self, request: Request) -> Result<Response> {
        let connection = self
            .endpoint
            .connect(server_addr(self.server), AUTH_ALPN)
            .await
            .context("could not reach the server")?;

        let (mut send, mut recv) = connection.open_bi().await?;
        write_frame(&mut send, &request).await?;
        send.finish()?;

        let response: Response = read_frame(&mut recv).await?;
        connection.close(0u32.into(), b"done");
        Ok(response)
    }

    pub async fn call_ok(&self, request: Request) -> Result<Response> {
        self.call(request).await?.into_result()
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub async fn close(self) {
        self.endpoint.close().await;
    }
}
