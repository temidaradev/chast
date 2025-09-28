use anyhow::Result;
use iroh::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub body: MessageBody,
    nonce: [u8; 16],
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MessageBody {
    AboutMe { from: NodeId, name: String },
    Message { from: NodeId, text: String },
    Disconnect { from: NodeId, name: String },
    MessageHistory { messages: Vec<StoredMessage> },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StoredMessage {
    pub from: NodeId,
    pub sender_name: String,
    pub text: String,
    pub timestamp: String,
}

impl Message {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(Into::into)
    }

    pub fn new(body: MessageBody) -> Self {
        Self {
            body,
            nonce: rand::random(),
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("serde_json::to_vec is infallible")
    }
}
