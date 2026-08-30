use anyhow::{Context, Result, bail};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use iroh::{NodeId, SecretKey};
use iroh_gossip::proto::TopicId;
use messaging::{
    Event,
    user::{
        Member, Membership, POWER_ADMIN, POWER_MEMBER, build_user_id, validate_localpart,
        validate_password,
    },
};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    str::FromStr,
    sync::{Arc, Mutex},
};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS identity (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    secret_key BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS rooms (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    topic BLOB NOT NULL,
    created_at TEXT NOT NULL,
    invite_only INTEGER NOT NULL DEFAULT 1,
    creator TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL UNIQUE,
    sender TEXT NOT NULL,
    sender_user TEXT NOT NULL DEFAULT '',
    sender_name TEXT NOT NULL,
    body TEXT NOT NULL,
    ts TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_events_room_seq ON events (room_id, seq);
CREATE TABLE IF NOT EXISTS users (
    user_id TEXT PRIMARY KEY,
    localpart TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    display_name TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS devices (
    device_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    node_id TEXT,
    created_at TEXT NOT NULL,
    last_seen TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_devices_user ON devices (user_id);
CREATE INDEX IF NOT EXISTS ix_devices_node ON devices (node_id);
CREATE TABLE IF NOT EXISTS memberships (
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    state TEXT NOT NULL,
    power_level INTEGER NOT NULL,
    invited_by TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (room_id, user_id)
);
CREATE INDEX IF NOT EXISTS ix_memberships_user ON memberships (user_id);
";

#[derive(Debug, Clone)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub topic: TopicId,
    pub created_at: String,
    pub invite_only: bool,
    pub creator: String,
}

#[derive(Debug, Clone)]
pub struct Device {
    pub device_id: String,
    pub user_id: String,
    pub device_name: String,
}

#[derive(Debug, Clone)]
pub struct User {
    pub user_id: String,
    pub localpart: String,
    pub display_name: String,
    pub created_at: String,
}

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Store")
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

fn random_hex(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    for byte in bytes.iter_mut() {
        *byte = rand::random();
    }
    hex(&bytes)
}

fn token_hash(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let existing: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;

    if !existing.iter().any(|name| name == column) {
        conn.execute_batch(&format!(
            "ALTER TABLE {} ADD COLUMN {} {}",
            table, column, definition
        ))?;
    }
    Ok(())
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database at {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.execute_batch(SCHEMA)?;

        ensure_column(&conn, "rooms", "invite_only", "INTEGER NOT NULL DEFAULT 1")?;
        ensure_column(&conn, "rooms", "creator", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "events", "sender_user", "TEXT NOT NULL DEFAULT ''")?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn secret_key(&self) -> Result<SecretKey> {
        let conn = self.conn.lock().unwrap();

        let existing: Option<Vec<u8>> = conn
            .query_row("SELECT secret_key FROM identity WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()?;

        if let Some(bytes) = existing {
            let bytes: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .ok()
                .context("stored identity key is not 32 bytes")?;
            return Ok(SecretKey::from_bytes(&bytes));
        }

        let bytes: [u8; 32] = rand::random();
        conn.execute(
            "INSERT INTO identity (id, secret_key) VALUES (1, ?1)",
            params![bytes.to_vec()],
        )?;
        Ok(SecretKey::from_bytes(&bytes))
    }

    pub fn server_name(&self, node_id: NodeId) -> Result<String> {
        let conn = self.conn.lock().unwrap();

        let existing: Option<String> = conn
            .query_row(
                "SELECT value FROM config WHERE key = 'server_name'",
                [],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(name) = existing {
            return Ok(name);
        }

        let name = std::env::var("P2P_CHAT_SERVER_NAME").unwrap_or_else(|_| node_id.fmt_short());
        conn.execute(
            "INSERT INTO config (key, value) VALUES ('server_name', ?1)",
            params![name],
        )?;
        Ok(name)
    }

    pub fn register_user(
        &self,
        localpart: &str,
        password: &str,
        server_name: &str,
    ) -> Result<User> {
        validate_localpart(localpart).map_err(|e| anyhow::anyhow!(e))?;
        validate_password(password).map_err(|e| anyhow::anyhow!(e))?;

        let user_id = build_user_id(localpart, server_name);
        let created_at = messaging::now_timestamp();

        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("failed to hash password: {}", e))?
            .to_string();

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (user_id, localpart, password_hash, display_name, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![user_id, localpart, password_hash, localpart, created_at],
        )
        .with_context(|| format!("user {} already exists", user_id))?;

        Ok(User {
            user_id,
            localpart: localpart.to_string(),
            display_name: localpart.to_string(),
            created_at,
        })
    }

    pub fn check_password(&self, localpart: &str, password: &str) -> Result<Option<User>> {
        let conn = self.conn.lock().unwrap();

        let row: Option<(String, String, String, String)> = conn
            .query_row(
                "SELECT user_id, password_hash, display_name, created_at FROM users
                 WHERE localpart = ?1",
                params![localpart],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;

        let Some((user_id, password_hash, display_name, created_at)) = row else {
            return Ok(None);
        };

        let parsed = PasswordHash::new(&password_hash)
            .map_err(|e| anyhow::anyhow!("stored password hash is invalid: {}", e))?;

        if Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_err()
        {
            return Ok(None);
        }

        Ok(Some(User {
            user_id,
            localpart: localpart.to_string(),
            display_name,
            created_at,
        }))
    }

    pub fn create_device(&self, user_id: &str, device_name: &str) -> Result<(Device, String)> {
        let device_id = random_hex(8);
        let token = random_hex(32);
        let now = messaging::now_timestamp();

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO devices (device_id, user_id, device_name, token_hash, node_id, created_at, last_seen)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?5)",
            params![device_id, user_id, device_name, token_hash(&token), now],
        )?;

        Ok((
            Device {
                device_id,
                user_id: user_id.to_string(),
                device_name: device_name.to_string(),
            },
            token,
        ))
    }

    pub fn forget_devices_for_node(&self, user_id: &str, node_id: NodeId) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute(
            "DELETE FROM devices WHERE user_id = ?1 AND node_id = ?2",
            params![user_id, node_id.to_string()],
        )?)
    }

    pub fn device_for_token(&self, token: &str) -> Result<Option<Device>> {
        let conn = self.conn.lock().unwrap();
        let row: Option<(String, String, String)> = conn
            .query_row(
                "SELECT device_id, user_id, device_name FROM devices WHERE token_hash = ?1",
                params![token_hash(token)],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        Ok(row.map(|(device_id, user_id, device_name)| Device {
            device_id,
            user_id,
            device_name,
        }))
    }

    pub fn revoke_token(&self, token: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "DELETE FROM devices WHERE token_hash = ?1",
            params![token_hash(token)],
        )?;
        Ok(changed > 0)
    }

    pub fn bind_device_node(&self, device_id: &str, node_id: NodeId) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE devices SET node_id = ?1, last_seen = ?2 WHERE device_id = ?3",
            params![node_id.to_string(), messaging::now_timestamp(), device_id],
        )?;
        Ok(())
    }

    pub fn user_for_node(&self, node_id: NodeId) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT user_id FROM devices WHERE node_id = ?1 ORDER BY last_seen DESC LIMIT 1",
                params![node_id.to_string()],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn find_user(&self, user_id: &str) -> Result<Option<User>> {
        let conn = self.conn.lock().unwrap();
        let row: Option<(String, String, String)> = conn
            .query_row(
                "SELECT localpart, display_name, created_at FROM users WHERE user_id = ?1",
                params![user_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        Ok(row.map(|(localpart, display_name, created_at)| User {
            user_id: user_id.to_string(),
            localpart,
            display_name,
            created_at,
        }))
    }

    pub fn list_users(&self) -> Result<Vec<User>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT user_id, localpart, display_name, created_at FROM users ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(User {
                user_id: row.get(0)?,
                localpart: row.get(1)?,
                display_name: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn set_display_name(&self, user_id: &str, display_name: &str) -> Result<()> {
        if display_name.trim().is_empty() {
            bail!("display name cannot be empty");
        }
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE users SET display_name = ?1 WHERE user_id = ?2",
            params![display_name, user_id],
        )?;
        Ok(())
    }

    pub fn display_name(&self, user_id: &str) -> Result<String> {
        Ok(self
            .find_user(user_id)?
            .map(|user| user.display_name)
            .unwrap_or_else(|| user_id.to_string()))
    }

    pub fn create_room(&self, name: &str, invite_only: bool, creator: &str) -> Result<Room> {
        if name.trim().is_empty() {
            bail!("room name cannot be empty");
        }

        let id = random_hex(8);
        let topic_bytes: [u8; 32] = rand::random();
        let created_at = messaging::now_timestamp();

        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO rooms (id, name, topic, created_at, invite_only, creator)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    name,
                    topic_bytes.to_vec(),
                    created_at,
                    invite_only as i64,
                    creator
                ],
            )
            .with_context(|| format!("a room named {} already exists", name))?;
        }

        if !creator.is_empty() {
            self.set_membership(&id, creator, Membership::Join, POWER_ADMIN, None)?;
        }

        Ok(Room {
            id,
            name: name.to_string(),
            topic: TopicId::from_bytes(topic_bytes),
            created_at,
            invite_only,
            creator: creator.to_string(),
        })
    }

    pub fn list_rooms(&self) -> Result<Vec<Room>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, topic, created_at, invite_only, creator FROM rooms ORDER BY created_at",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;

        let mut rooms = Vec::new();
        for row in rows {
            let (id, name, topic, created_at, invite_only, creator) = row?;
            rooms.push(Room {
                id,
                name,
                topic: TopicId::from_bytes(
                    topic
                        .as_slice()
                        .try_into()
                        .ok()
                        .context("stored topic is not 32 bytes")?,
                ),
                created_at,
                invite_only: invite_only != 0,
                creator,
            });
        }
        Ok(rooms)
    }

    pub fn find_room(&self, needle: &str) -> Result<Option<Room>> {
        Ok(self
            .list_rooms()?
            .into_iter()
            .find(|room| room.id == needle || room.name == needle))
    }

    pub fn delete_room(&self, room_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM events WHERE room_id = ?1", params![room_id])?;
        conn.execute(
            "DELETE FROM memberships WHERE room_id = ?1",
            params![room_id],
        )?;
        conn.execute("DELETE FROM rooms WHERE id = ?1", params![room_id])?;
        Ok(())
    }

    pub fn membership(&self, room_id: &str, user_id: &str) -> Result<Option<(Membership, i64)>> {
        let conn = self.conn.lock().unwrap();
        let row: Option<(String, i64)> = conn
            .query_row(
                "SELECT state, power_level FROM memberships WHERE room_id = ?1 AND user_id = ?2",
                params![room_id, user_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        Ok(row.and_then(|(state, power)| Membership::parse(&state).map(|state| (state, power))))
    }

    pub fn power_level(&self, room_id: &str, user_id: &str) -> Result<i64> {
        Ok(self
            .membership(room_id, user_id)?
            .map(|(_, power)| power)
            .unwrap_or(POWER_MEMBER))
    }

    pub fn is_joined(&self, room_id: &str, user_id: &str) -> Result<bool> {
        Ok(matches!(
            self.membership(room_id, user_id)?,
            Some((Membership::Join, _))
        ))
    }

    pub fn set_membership(
        &self,
        room_id: &str,
        user_id: &str,
        state: Membership,
        power_level: i64,
        invited_by: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO memberships (room_id, user_id, state, power_level, invited_by, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (room_id, user_id) DO UPDATE SET
                state = excluded.state,
                power_level = excluded.power_level,
                invited_by = COALESCE(excluded.invited_by, memberships.invited_by),
                updated_at = excluded.updated_at",
            params![
                room_id,
                user_id,
                state.as_str(),
                power_level,
                invited_by,
                messaging::now_timestamp()
            ],
        )?;
        Ok(())
    }

    pub fn members(&self, room_id: &str) -> Result<Vec<Member>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT m.user_id, COALESCE(u.display_name, m.user_id), m.state, m.power_level
             FROM memberships m LEFT JOIN users u ON u.user_id = m.user_id
             WHERE m.room_id = ?1 ORDER BY m.power_level DESC, m.user_id",
        )?;

        let rows = stmt.query_map(params![room_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;

        let mut members = Vec::new();
        for row in rows {
            let (user_id, display_name, state, power_level) = row?;
            members.push(Member {
                user_id,
                display_name,
                membership: Membership::parse(&state).unwrap_or(Membership::Leave),
                power_level,
            });
        }
        Ok(members)
    }

    pub fn joined_count(&self, room_id: &str) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memberships WHERE room_id = ?1 AND state = 'join'",
            params![room_id],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn rooms_for_user(&self, user_id: &str) -> Result<Vec<(Room, Membership, i64)>> {
        let mut out = Vec::new();
        for room in self.list_rooms()? {
            if let Some((state, power)) = self.membership(&room.id, user_id)? {
                if state != Membership::Leave {
                    out.push((room, state, power));
                }
            }
        }
        Ok(out)
    }

    pub fn append_event(
        &self,
        room_id: &str,
        event_id: &str,
        sender: NodeId,
        sender_user: &str,
        sender_name: &str,
        text: &str,
        timestamp: &str,
    ) -> Result<Option<Event>> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "INSERT OR IGNORE INTO events
             (room_id, event_id, sender, sender_user, sender_name, body, ts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                room_id,
                event_id,
                sender.to_string(),
                sender_user,
                sender_name,
                text,
                timestamp
            ],
        )?;

        if changed == 0 {
            return Ok(None);
        }

        Ok(Some(Event {
            id: event_id.to_string(),
            seq: conn.last_insert_rowid() as u64,
            sender,
            sender_user: sender_user.to_string(),
            sender_name: sender_name.to_string(),
            text: text.to_string(),
            timestamp: timestamp.to_string(),
        }))
    }

    pub fn events_since(&self, room_id: &str, since_seq: u64, limit: usize) -> Result<Vec<Event>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT seq, event_id, sender, sender_user, sender_name, body, ts FROM events
             WHERE room_id = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3",
        )?;

        let rows = stmt.query_map(params![room_id, since_seq as i64, limit as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;

        let mut events = Vec::new();
        for row in rows {
            let (seq, id, sender, sender_user, sender_name, text, timestamp) = row?;
            events.push(Event {
                id,
                seq: seq as u64,
                sender: NodeId::from_str(&sender).context("stored sender is not a valid node id")?,
                sender_user,
                sender_name,
                text,
                timestamp,
            });
        }
        Ok(events)
    }

    pub fn latest_seq(&self, room_id: &str) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let seq: Option<i64> = conn.query_row(
            "SELECT MAX(seq) FROM events WHERE room_id = ?1",
            params![room_id],
            |row| row.get(0),
        )?;
        Ok(seq.unwrap_or(0) as u64)
    }

    pub fn event_count(&self, room_id: &str) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM events WHERE room_id = ?1",
            params![room_id],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_db() -> PathBuf {
        let mut path = std::env::temp_dir();
        let suffix: [u8; 8] = rand::random();
        let mut name = String::from("p2p-chat-store-");
        for byte in suffix {
            name.push_str(&format!("{:02x}", byte));
        }
        name.push_str(".db");
        path.push(name);
        path
    }

    fn node() -> NodeId {
        SecretKey::from_bytes(&rand::random()).public()
    }

    #[test]
    fn identity_and_rooms_survive_reopen() {
        let path = temp_db();

        let (key, room) = {
            let store = Store::open(&path).unwrap();
            let key = store.secret_key().unwrap();
            let room = store.create_room("general", true, "").unwrap();
            (key.to_bytes(), room)
        };

        let store = Store::open(&path).unwrap();
        assert_eq!(store.secret_key().unwrap().to_bytes(), key);

        let rooms = store.list_rooms().unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].id, room.id);
        assert_eq!(rooms[0].topic, room.topic);
        assert!(rooms[0].invite_only);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn events_are_deduplicated_and_resumable() {
        let path = temp_db();
        let store = Store::open(&path).unwrap();
        let room = store.create_room("general", true, "").unwrap();
        let from = node();

        let first = store
            .append_event(&room.id, "a", from, "@ana:s", "ana", "one", "t0")
            .unwrap()
            .unwrap();
        let second = store
            .append_event(&room.id, "b", from, "@ana:s", "ana", "two", "t1")
            .unwrap()
            .unwrap();

        assert!(second.seq > first.seq);
        assert!(
            store
                .append_event(&room.id, "a", from, "@ana:s", "ana", "one", "t0")
                .unwrap()
                .is_none()
        );
        assert_eq!(store.event_count(&room.id).unwrap(), 2);

        let tail = store.events_since(&room.id, first.seq, 10).unwrap();
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].id, "b");
        assert_eq!(tail[0].sender_user, "@ana:s");

        assert_eq!(store.latest_seq(&room.id).unwrap(), second.seq);
        assert!(
            store
                .events_since(&room.id, second.seq, 10)
                .unwrap()
                .is_empty()
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn history_survives_reopen_and_rooms_stay_isolated() {
        let path = temp_db();
        let from = node();

        let (general, dev) = {
            let store = Store::open(&path).unwrap();
            let general = store.create_room("general", true, "").unwrap();
            let dev = store.create_room("dev", true, "").unwrap();
            store
                .append_event(&general.id, "a", from, "@ana:s", "ana", "hello", "t0")
                .unwrap();
            store
                .append_event(&dev.id, "b", from, "@ana:s", "ana", "deploying", "t1")
                .unwrap();
            (general, dev)
        };

        let store = Store::open(&path).unwrap();
        let restored = store.events_since(&general.id, 0, 10).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].text, "hello");
        assert_eq!(store.event_count(&dev.id).unwrap(), 1);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn sync_chunking_walks_the_whole_log() {
        let path = temp_db();
        let store = Store::open(&path).unwrap();
        let room = store.create_room("general", true, "").unwrap();
        let from = node();

        for index in 0..10 {
            store
                .append_event(
                    &room.id,
                    &format!("e{}", index),
                    from,
                    "@ana:s",
                    "ana",
                    "x",
                    "t",
                )
                .unwrap();
        }

        let latest = store.latest_seq(&room.id).unwrap();
        let mut cursor = 0;
        let mut seen = 0;

        while cursor < latest {
            let batch = store.events_since(&room.id, cursor, 3).unwrap();
            assert!(!batch.is_empty());
            seen += batch.len();
            cursor = batch.last().unwrap().seq;
        }

        assert_eq!(seen, 10);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn duplicate_room_names_are_rejected() {
        let path = temp_db();
        let store = Store::open(&path).unwrap();
        store.create_room("general", true, "").unwrap();
        assert!(store.create_room("general", true, "").is_err());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn an_old_database_gains_the_new_columns() {
        let path = temp_db();

        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE rooms (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    topic BLOB NOT NULL,
                    created_at TEXT NOT NULL
                 );
                 CREATE TABLE events (
                    seq INTEGER PRIMARY KEY AUTOINCREMENT,
                    room_id TEXT NOT NULL,
                    event_id TEXT NOT NULL UNIQUE,
                    sender TEXT NOT NULL,
                    sender_name TEXT NOT NULL,
                    body TEXT NOT NULL,
                    ts TEXT NOT NULL
                 );",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO rooms (id, name, topic, created_at) VALUES ('r1', 'old', ?1, 't')",
                params![vec![7u8; 32]],
            )
            .unwrap();
        }

        let store = Store::open(&path).unwrap();
        let rooms = store.list_rooms().unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].name, "old");
        assert!(rooms[0].invite_only);

        store
            .append_event(&rooms[0].id, "e", node(), "@ana:s", "ana", "hi", "t")
            .unwrap()
            .unwrap();
        assert_eq!(store.event_count(&rooms[0].id).unwrap(), 1);

        std::fs::remove_file(&path).ok();
    }
}
