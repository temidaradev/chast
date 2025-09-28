use iroh::{Endpoint, NodeId, protocol::Router};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::app::types;

#[derive(Clone)]
pub struct AppState {
    pub sender: Option<iroh_gossip::api::GossipSender>,
    pub endpoint: Option<Endpoint>,
    pub router: Option<Router>,
    pub current_username: String,
    pub current_node_id: Option<NodeId>,
    pub current_session_token: Option<String>,
    pub names: Arc<Mutex<HashMap<NodeId, String>>>,
    pub messages: Arc<Mutex<Vec<types::ChatMessage>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sender: None,
            endpoint: None,
            router: None,
            current_username: String::new(),
            current_node_id: None,
            current_session_token: None,
            names: Arc::new(Mutex::new(HashMap::new())),
            messages: Arc::new(Mutex::new(Vec::<types::ChatMessage>::new())),
        }
    }
}
