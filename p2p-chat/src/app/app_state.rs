use client::RoomHistory;
use iroh::{NodeId, SecretKey};
use iroh_gossip::proto::TopicId;
use messaging::Membership;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::app::types;

#[derive(Clone, Debug)]
pub struct RosterEntry {
    pub user_id: String,
    pub display_name: String,
    pub power_level: i64,
    pub membership: Membership,
}

#[derive(Clone)]
pub struct AppState {
    pub sender: Option<iroh_gossip::api::GossipSender>,
    pub secret_key: Option<SecretKey>,
    pub current_node_id: Option<NodeId>,
    pub server_node_id: Option<NodeId>,
    pub current_topic: Option<TopicId>,
    pub current_session_token: Option<String>,
    pub current_room: String,
    pub my_user_id: String,
    pub display_name: String,
    pub my_power: i64,
    pub roster: Arc<Mutex<HashMap<String, RosterEntry>>>,
    pub nodes: Arc<Mutex<HashMap<NodeId, String>>>,
    pub messages: Arc<Mutex<Vec<types::ChatMessage>>>,
    pub history: Arc<Mutex<RoomHistory>>,
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
            secret_key: None,
            current_node_id: None,
            server_node_id: None,
            current_topic: None,
            current_session_token: None,
            current_room: String::new(),
            my_user_id: String::new(),
            display_name: String::new(),
            my_power: 0,
            roster: Arc::new(Mutex::new(HashMap::new())),
            nodes: Arc::new(Mutex::new(HashMap::new())),
            messages: Arc::new(Mutex::new(Vec::new())),
            history: Arc::new(Mutex::new(RoomHistory::default())),
        }
    }
}
