use anyhow::{Context, Result};
use iroh_gossip::proto::TopicId;
use messaging::Event;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, path::PathBuf};

use crate::identity::config_dir;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RoomHistory {
    pub ticket: String,
    pub last_seq: u64,
    pub events: Vec<Event>,
    #[serde(skip)]
    seen: HashSet<String>,
}

impl RoomHistory {
    pub fn load(topic: &TopicId, ticket: &str) -> Self {
        let mut history = match path_for(topic).and_then(read_file) {
            Ok(Some(history)) => history,
            Ok(None) => Self::default(),
            Err(e) => {
                eprintln!("> could not read stored history: {}", e);
                Self::default()
            }
        };

        history.ticket = ticket.to_string();
        history.seen = history.events.iter().map(|e| e.id.clone()).collect();
        history
    }

    pub fn contains(&self, event_id: &str) -> bool {
        self.seen.contains(event_id)
    }

    pub fn record(&mut self, event: Event) -> bool {
        if !self.seen.insert(event.id.clone()) {
            if event.seq > 0 {
                if let Some(existing) = self.events.iter_mut().find(|e| e.id == event.id) {
                    existing.seq = event.seq;
                }
            }
            return false;
        }

        self.events.push(event);
        true
    }

    pub fn advance(&mut self, latest_seq: u64) {
        self.last_seq = self.last_seq.max(latest_seq);
    }

    pub fn save(&self, topic: &TopicId) -> Result<()> {
        let path = path_for(topic)?;
        let json = serde_json::to_string_pretty(self)?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, json)
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        fs::rename(&temporary, &path)
            .with_context(|| format!("failed to replace {}", path.display()))?;
        Ok(())
    }
}

fn read_file(path: PathBuf) -> Result<Option<RoomHistory>> {
    if !path.exists() {
        return Ok(None);
    }

    let contents =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let history = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(history))
}

fn path_for(topic: &TopicId) -> Result<PathBuf> {
    let dir = config_dir()?.join("rooms");
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;

    let mut name = String::with_capacity(64);
    for byte in topic.as_bytes() {
        name.push_str(&format!("{:02x}", byte));
    }
    Ok(dir.join(format!("{}.json", name)))
}
