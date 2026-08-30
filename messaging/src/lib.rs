pub mod rpc;
pub mod user;

use anyhow::{Context, Result};
use ed25519_dalek::Signature;
use iroh::{NodeId, SecretKey};
use serde::{Deserialize, Serialize};

pub use user::{Member, Membership};

pub const SYNC_CHUNK: usize = 256;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MessageBody {
    Hello,
    Goodbye,
    Introduce {
        display_name: String,
    },
    Message {
        id: String,
        text: String,
    },
    SyncRequest {
        since_seq: u64,
    },
    SyncResponse {
        to: NodeId,
        events: Vec<Event>,
        latest_seq: u64,
    },
    Presence {
        node_id: NodeId,
        user_id: String,
        display_name: String,
        power_level: i64,
        membership: Membership,
    },
    Rejected {
        to: NodeId,
        reason: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Event {
    pub id: String,
    pub seq: u64,
    pub sender: NodeId,
    pub sender_user: String,
    pub sender_name: String,
    pub text: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Envelope {
    from: NodeId,
    nonce: [u8; 16],
    body: MessageBody,
    signature: Vec<u8>,
}

fn signing_payload(nonce: &[u8; 16], body: &MessageBody) -> Result<Vec<u8>> {
    serde_json::to_vec(&(nonce, body)).map_err(Into::into)
}

pub fn sign(secret_key: &SecretKey, body: MessageBody) -> Result<Vec<u8>> {
    let nonce: [u8; 16] = rand::random();
    let payload = signing_payload(&nonce, &body)?;
    let signature = secret_key.sign(&payload).to_bytes().to_vec();

    let envelope = Envelope {
        from: secret_key.public(),
        nonce,
        body,
        signature,
    };
    serde_json::to_vec(&envelope).map_err(Into::into)
}

pub fn verify(bytes: &[u8]) -> Result<(NodeId, MessageBody)> {
    let envelope: Envelope = serde_json::from_slice(bytes)?;

    let signature: [u8; 64] = envelope
        .signature
        .as_slice()
        .try_into()
        .ok()
        .context("signature is not 64 bytes")?;
    let signature = Signature::from_bytes(&signature);

    let payload = signing_payload(&envelope.nonce, &envelope.body)?;
    envelope
        .from
        .verify(&payload, &signature)
        .context("signature does not match the sending node")?;

    Ok((envelope.from, envelope.body))
}

pub fn new_event_id() -> String {
    let bytes: [u8; 16] = rand::random();
    let mut out = String::with_capacity(32);
    for byte in bytes {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

pub fn now_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> SecretKey {
        SecretKey::from_bytes(&rand::random())
    }

    #[test]
    fn round_trip_recovers_sender_and_body() {
        let secret = key();
        let encoded = sign(
            &secret,
            MessageBody::Message {
                id: "abc".to_string(),
                text: "hello".to_string(),
            },
        )
        .unwrap();

        let (from, body) = verify(&encoded).unwrap();
        assert_eq!(from, secret.public());
        match body {
            MessageBody::Message { id, text } => {
                assert_eq!(id, "abc");
                assert_eq!(text, "hello");
            }
            other => panic!("unexpected body {:?}", other),
        }
    }

    #[test]
    fn a_tampered_body_fails_verification() {
        let secret = key();
        let encoded = sign(
            &secret,
            MessageBody::Message {
                id: "abc".to_string(),
                text: "hello".to_string(),
            },
        )
        .unwrap();

        let mut envelope: Envelope = serde_json::from_slice(&encoded).unwrap();
        envelope.body = MessageBody::Message {
            id: "abc".to_string(),
            text: "goodbye".to_string(),
        };
        let tampered = serde_json::to_vec(&envelope).unwrap();

        assert!(verify(&tampered).is_err());
    }

    #[test]
    fn a_forged_sender_fails_verification() {
        let secret = key();
        let attacker = key();
        let encoded = sign(&secret, MessageBody::Hello).unwrap();

        let mut envelope: Envelope = serde_json::from_slice(&encoded).unwrap();
        envelope.from = attacker.public();
        let forged = serde_json::to_vec(&envelope).unwrap();

        assert!(verify(&forged).is_err());
    }

    #[test]
    fn a_replayed_signature_does_not_transfer_to_another_body() {
        let secret = key();
        let first = sign(&secret, MessageBody::Hello).unwrap();
        let second = sign(&secret, MessageBody::Goodbye).unwrap();

        let first: Envelope = serde_json::from_slice(&first).unwrap();
        let mut second: Envelope = serde_json::from_slice(&second).unwrap();
        second.signature = first.signature;

        assert!(verify(&serde_json::to_vec(&second).unwrap()).is_err());
    }
}
