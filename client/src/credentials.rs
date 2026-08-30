use anyhow::{Context, Result, bail};
use iroh::NodeId;
use serde::{Deserialize, Serialize};
use std::{fs, str::FromStr};
use ticket::Ticket;

use crate::identity::{config_dir, write_private};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Credentials {
    pub server_node_id: Option<String>,
    pub server_name: Option<String>,
    pub user_id: Option<String>,
    pub device_id: Option<String>,
    pub display_name: Option<String>,
    pub token: Option<String>,
    pub last_ticket: Option<String>,
}

impl Credentials {
    pub fn load() -> Result<Self> {
        let path = config_dir()?.join("session.json");
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let path = config_dir()?.join("session.json");
        let json = serde_json::to_vec_pretty(self)?;
        write_private(&path, &json)
    }

    pub fn clear_session(&mut self) {
        self.user_id = None;
        self.device_id = None;
        self.display_name = None;
        self.token = None;
    }

    pub fn remember_server(&mut self, input: &str) -> Result<()> {
        let input = input.trim();

        if let Ok(node_id) = NodeId::from_str(input) {
            self.server_node_id = Some(node_id.to_string());
            return Ok(());
        }

        let ticket = Ticket::from_str(input)
            .map_err(|_| anyhow::anyhow!("that is neither a server address nor a room ticket"))?;
        let node = ticket
            .nodes
            .first()
            .context("that ticket does not name a server")?;

        self.server_node_id = Some(node.node_id.to_string());
        self.last_ticket = Some(input.to_string());
        Ok(())
    }

    pub fn server(&self) -> Result<NodeId> {
        let raw = self
            .server_node_id
            .as_deref()
            .context("no server known yet, run: chatctl connect <ticket>")?;
        NodeId::from_str(raw).context("the stored server id is not valid")
    }

    pub fn token(&self) -> Result<&str> {
        match self.token.as_deref() {
            Some(token) if !token.is_empty() => Ok(token),
            _ => bail!("you are not logged in, run: chatctl login <user> <password>"),
        }
    }
}
