mod auth;
mod perms;
mod store;

use anyhow::{Context, Result};
use auth::{AuthProtocol, describe_member};
use futures_lite::StreamExt;
use iroh::{Endpoint, NodeAddr, NodeId, SecretKey, protocol::Router};
use iroh_gossip::{api::Event as GossipEvent, net::Gossip};
use messaging::{
    MessageBody, SYNC_CHUNK,
    rpc::AUTH_ALPN,
    user::{Membership, POWER_MEMBER, build_user_id, power_label},
};
use std::path::{Path, PathBuf};
use store::{Room, Store};
use ticket::Ticket;

const DEFAULT_RELAY_URL: &str = "https://relay.iroh.link";
const DEFAULT_DB: &str = "server.db";

pub fn node_addr(node_id: NodeId) -> NodeAddr {
    let relay_url = DEFAULT_RELAY_URL.parse().expect("invalid relay url");
    NodeAddr::new(node_id).with_relay_url(relay_url)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db_path =
        PathBuf::from(std::env::var("P2P_CHAT_DB").unwrap_or_else(|_| DEFAULT_DB.to_string()));

    let store = Store::open(&db_path)?;
    let secret_key = store.secret_key()?;
    let node_id = secret_key.public();
    let server_name = store.server_name(node_id)?;

    match args.first().map(String::as_str) {
        Some("create") => {
            let flags: Vec<&String> = args.iter().filter(|a| a.starts_with("--")).collect();
            let positional: Vec<&String> = args
                .iter()
                .skip(1)
                .filter(|a| !a.starts_with("--"))
                .collect();

            let name = positional
                .first()
                .context("usage: server create <room-name> [owner] [--open]")?;
            let invite_only = !flags.iter().any(|f| f.as_str() == "--open");

            let owner = match positional.get(1) {
                Some(localpart) => {
                    let user_id = build_user_id(localpart, &server_name);
                    store
                        .find_user(&user_id)?
                        .with_context(|| format!("no user called {}", user_id))?
                        .user_id
                }
                None => String::new(),
            };

            let room = store.create_room(name, invite_only, &owner)?;
            println!(
                "Created room \"{}\" ({}), {}",
                room.name,
                room.id,
                if invite_only { "invite only" } else { "open" }
            );
            if !owner.is_empty() {
                println!("Owner {} starts as admin", owner);
            }
            print_ticket(&room, node_id);
            Ok(())
        }

        Some("list") => {
            let rooms = store.list_rooms()?;
            if rooms.is_empty() {
                println!("No rooms yet. Create one with: server create <room-name>");
                return Ok(());
            }
            println!("Server {} ({})", server_name, node_id);
            for room in rooms {
                println!();
                println!(
                    "\"{}\" ({}) - {} messages, {} member(s), {}",
                    room.name,
                    room.id,
                    store.event_count(&room.id)?,
                    store.joined_count(&room.id)?,
                    if room.invite_only {
                        "invite only"
                    } else {
                        "open"
                    }
                );
                print_ticket(&room, node_id);
            }
            Ok(())
        }

        Some("delete") => {
            let needle = args.get(1).context("usage: server delete <room-name|id>")?;
            let room = store
                .find_room(needle)?
                .with_context(|| format!("no room matching {}", needle))?;
            store.delete_room(&room.id)?;
            println!("Deleted room \"{}\" ({})", room.name, room.id);
            Ok(())
        }

        Some("register") => {
            let localpart = args
                .get(1)
                .context("usage: server register <username> <password>")?;
            let password = args
                .get(2)
                .context("usage: server register <username> <password>")?;
            let user = store.register_user(localpart, password, &server_name)?;
            println!("Registered {}", user.user_id);
            Ok(())
        }

        Some("users") => {
            let users = store.list_users()?;
            if users.is_empty() {
                println!("No users yet. Create one with: server register <username> <password>");
                return Ok(());
            }
            for user in users {
                println!("{:<28} {}", user.user_id, user.display_name);
            }
            Ok(())
        }

        Some("members") => {
            let needle = args.get(1).context("usage: server members <room>")?;
            let room = store
                .find_room(needle)?
                .with_context(|| format!("no room matching {}", needle))?;
            let members = store.members(&room.id)?;
            if members.is_empty() {
                println!("\"{}\" has no members yet", room.name);
                return Ok(());
            }
            for member in members {
                println!("{}", describe_member(&member));
            }
            Ok(())
        }

        Some("grant") => {
            let needle = args
                .get(1)
                .context("usage: server grant <room> <username> <level>")?;
            let localpart = args
                .get(2)
                .context("usage: server grant <room> <username> <level>")?;
            let level: i64 = args
                .get(3)
                .context("usage: server grant <room> <username> <level>")?
                .parse()
                .context("level must be a number, for example 100 or 50")?;

            let room = store
                .find_room(needle)?
                .with_context(|| format!("no room matching {}", needle))?;
            let user_id = build_user_id(localpart, &server_name);
            store
                .find_user(&user_id)?
                .with_context(|| format!("no user called {}", user_id))?;

            let state = store
                .membership(&room.id, &user_id)?
                .map(|(state, _)| state)
                .unwrap_or(Membership::Join);
            store.set_membership(&room.id, &user_id, state, level, None)?;
            println!(
                "{} is now {} ({}) in \"{}\"",
                user_id,
                power_label(level),
                level,
                room.name
            );
            Ok(())
        }

        Some("invite") => {
            let needle = args.get(1).context("usage: server invite <room> <username>")?;
            let localpart = args.get(2).context("usage: server invite <room> <username>")?;

            let room = store
                .find_room(needle)?
                .with_context(|| format!("no room matching {}", needle))?;
            let user_id = build_user_id(localpart, &server_name);
            store
                .find_user(&user_id)?
                .with_context(|| format!("no user called {}", user_id))?;

            store.set_membership(&room.id, &user_id, Membership::Invite, POWER_MEMBER, None)?;
            println!("Invited {} to \"{}\"", user_id, room.name);
            Ok(())
        }

        Some("serve") | None => serve(store, secret_key, server_name, &db_path).await,

        Some(other) => {
            eprintln!("unknown command: {}", other);
            usage();
            std::process::exit(2);
        }
    }
}

fn usage() {
    eprintln!("usage:");
    eprintln!("  server serve                          host every room (default)");
    eprintln!("  server register <user> <password>     create an account");
    eprintln!("  server users                          list accounts");
    eprintln!("  server create <room> [owner] [--open] create a room, owner becomes admin");
    eprintln!("  server list                           list rooms and their tickets");
    eprintln!("  server members <room>                 list room members and power levels");
    eprintln!("  server invite <room> <user>           invite a user");
    eprintln!("  server grant <room> <user> <level>    set a power level, 100 admin, 50 moderator");
    eprintln!("  server delete <room>                  delete a room and its history");
    eprintln!();
    eprintln!("database path comes from P2P_CHAT_DB, default {}", DEFAULT_DB);
    eprintln!("server name comes from P2P_CHAT_SERVER_NAME on first run");
}

fn ticket_for(topic: iroh_gossip::proto::TopicId, node_id: NodeId) -> Ticket {
    Ticket {
        topic,
        nodes: vec![node_addr(node_id)],
    }
}

fn print_ticket(room: &Room, node_id: NodeId) {
    println!("{}", "=".repeat(60));
    println!("{}", ticket_for(room.topic, node_id));
    println!("{}", "=".repeat(60));
}

async fn serve(
    store: Store,
    secret_key: SecretKey,
    server_name: String,
    db_path: &Path,
) -> Result<()> {
    let rooms = store.list_rooms()?;

    let endpoint = Endpoint::builder()
        .secret_key(secret_key.clone())
        .discovery_n0()
        .bind()
        .await?;
    let node_id = endpoint.node_id();

    let gossip = Gossip::builder().spawn(endpoint.clone());
    let (new_rooms, mut incoming_rooms) = tokio::sync::mpsc::unbounded_channel::<Room>();
    let auth = AuthProtocol::new(store.clone(), server_name.clone(), node_id, new_rooms);

    let _router = Router::builder(endpoint.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .accept(AUTH_ALPN, auth)
        .spawn();

    {
        let gossip = gossip.clone();
        let store = store.clone();
        let secret_key = secret_key.clone();

        tokio::spawn(async move {
            while let Some(room) = incoming_rooms.recv().await {
                println!();
                println!("Now hosting \"{}\" ({})", room.name, room.id);
                print_ticket(&room, node_id);

                let gossip = gossip.clone();
                let store = store.clone();
                let secret_key = secret_key.clone();

                tokio::spawn(async move {
                    let name = room.name.clone();
                    if let Err(e) = host_room(room, gossip, store, secret_key).await {
                        eprintln!("[{}] room stopped: {}", name, e);
                    }
                });
            }
        });
    }

    println!("Server {}", server_name);
    println!("Database: {}", db_path.display());
    println!("Accounts: {}", store.list_users()?.len());
    println!();
    println!("Share this address so people can sign up and log in:");
    println!("{}", "=".repeat(60));
    println!("{}", node_id);
    println!("{}", "=".repeat(60));

    if rooms.is_empty() {
        println!("No rooms yet. Create one with: server create <room-name>");
    } else {
        println!("Hosting {} room(s):", rooms.len());
        for room in &rooms {
            println!();
            println!(
                "\"{}\" ({}) - {} messages, {} member(s), {}",
                room.name,
                room.id,
                store.event_count(&room.id)?,
                store.joined_count(&room.id)?,
                if room.invite_only {
                    "invite only"
                } else {
                    "open"
                }
            );
            print_ticket(room, node_id);
        }
    }

    println!();
    println!("Tickets stay the same across restarts. Ctrl-C to stop.");
    println!("{}", "-".repeat(60));

    for room in rooms {
        let gossip = gossip.clone();
        let store = store.clone();
        let secret_key = secret_key.clone();
        tokio::spawn(async move {
            let name = room.name.clone();
            if let Err(e) = host_room(room, gossip, store, secret_key).await {
                eprintln!("[{}] room stopped: {}", name, e);
            }
        });
    }

    tokio::signal::ctrl_c().await?;
    println!();
    println!("Shutting down.");
    Ok(())
}

async fn host_room(
    room: Room,
    gossip: Gossip,
    store: Store,
    secret_key: SecretKey,
) -> Result<()> {
    let subscription = gossip.subscribe(room.topic, vec![]).await?;
    let (sender, mut receiver) = subscription.split();

    let broadcast = |body: MessageBody| {
        let secret_key = secret_key.clone();
        let sender = sender.clone();
        async move {
            match messaging::sign(&secret_key, body) {
                Ok(bytes) => {
                    if let Err(e) = sender.broadcast(bytes.into()).await {
                        eprintln!("failed to broadcast: {}", e);
                    }
                }
                Err(e) => eprintln!("failed to sign message: {}", e),
            }
        }
    };

    while let Some(event) = receiver.try_next().await? {
        let GossipEvent::Received(msg) = event else {
            continue;
        };

        let (from, body) = match messaging::verify(&msg.content) {
            Ok(verified) => verified,
            Err(e) => {
                eprintln!("[{}] rejected an unsigned message: {}", room.name, e);
                continue;
            }
        };

        if from == secret_key.public() {
            continue;
        }

        let user_id = store.user_for_node(from)?;

        match body {
            MessageBody::Hello => {
                let Some(user_id) = user_id else {
                    println!(
                        "[{}] refused unknown device {}",
                        room.name,
                        from.fmt_short()
                    );
                    broadcast(MessageBody::Rejected {
                        to: from,
                        reason: "log in before joining this room".to_string(),
                    })
                    .await;
                    continue;
                };

                let Some((membership, power_level)) = store.membership(&room.id, &user_id)? else {
                    broadcast(MessageBody::Rejected {
                        to: from,
                        reason: "you are not a member of this room".to_string(),
                    })
                    .await;
                    continue;
                };

                if membership != Membership::Join {
                    broadcast(MessageBody::Rejected {
                        to: from,
                        reason: format!("your membership here is {}", membership.as_str()),
                    })
                    .await;
                    continue;
                }

                let display_name = store.display_name(&user_id)?;
                println!(
                    "[{}] {} ({}) online as {}",
                    room.name,
                    user_id,
                    power_label(power_level),
                    display_name
                );

                broadcast(MessageBody::Presence {
                    node_id: from,
                    user_id,
                    display_name,
                    power_level,
                    membership,
                })
                .await;
            }

            MessageBody::Goodbye => {
                if let Some(user_id) = user_id {
                    println!("[{}] {} went offline", room.name, user_id);
                    let display_name = store.display_name(&user_id)?;
                    let power_level = store.power_level(&room.id, &user_id)?;
                    broadcast(MessageBody::Presence {
                        node_id: from,
                        user_id,
                        display_name,
                        power_level,
                        membership: Membership::Leave,
                    })
                    .await;
                }
            }

            MessageBody::Message { id, text } => {
                let Some(user_id) = user_id else {
                    broadcast(MessageBody::Rejected {
                        to: from,
                        reason: "log in before posting here".to_string(),
                    })
                    .await;
                    continue;
                };

                let membership = store
                    .membership(&room.id, &user_id)?
                    .map(|(state, _)| state);
                if !perms::can_speak(membership) {
                    println!(
                        "[{}] dropped a message from non-member {}",
                        room.name, user_id
                    );
                    broadcast(MessageBody::Rejected {
                        to: from,
                        reason: match membership {
                            Some(state) => {
                                format!("your membership here is {}", state.as_str())
                            }
                            None => "you are not a member of this room".to_string(),
                        },
                    })
                    .await;
                    continue;
                }

                let display_name = store.display_name(&user_id)?;
                let timestamp = messaging::now_timestamp();

                if let Some(event) = store.append_event(
                    &room.id,
                    &id,
                    from,
                    &user_id,
                    &display_name,
                    &text,
                    &timestamp,
                )? {
                    println!("[{}] #{} {}: {}", room.name, event.seq, user_id, text);
                }
            }

            MessageBody::SyncRequest { since_seq } => {
                let Some(user_id) = user_id else {
                    broadcast(MessageBody::Rejected {
                        to: from,
                        reason: "log in before reading this room".to_string(),
                    })
                    .await;
                    continue;
                };

                if !store.is_joined(&room.id, &user_id)? {
                    broadcast(MessageBody::Rejected {
                        to: from,
                        reason: "you are not a member of this room".to_string(),
                    })
                    .await;
                    continue;
                }

                let events = store.events_since(&room.id, since_seq, SYNC_CHUNK)?;
                let latest_seq = store.latest_seq(&room.id)?;

                println!(
                    "[{}] sync for {} from #{}, sending {} event(s) up to #{}",
                    room.name,
                    user_id,
                    since_seq,
                    events.len(),
                    latest_seq
                );

                broadcast(MessageBody::SyncResponse {
                    to: from,
                    events,
                    latest_seq,
                })
                .await;
            }

            MessageBody::Introduce { .. }
            | MessageBody::Presence { .. }
            | MessageBody::SyncResponse { .. }
            | MessageBody::Rejected { .. } => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use messaging::user::POWER_ADMIN;

    fn temp_store() -> (Store, String, PathBuf) {
        let mut path = std::env::temp_dir();
        let suffix: [u8; 8] = rand::random();
        let mut name = String::from("p2p-chat-users-");
        for byte in suffix {
            name.push_str(&format!("{:02x}", byte));
        }
        name.push_str(".db");
        path.push(name);

        let store = Store::open(&path).unwrap();
        let node_id = store.secret_key().unwrap().public();
        let server_name = store.server_name(node_id).unwrap();
        (store, server_name, path)
    }

    #[test]
    fn passwords_are_hashed_and_verified() {
        let (store, server_name, _path) = temp_store();
        let user = store
            .register_user("alice", "correct horse", &server_name)
            .unwrap();
        assert_eq!(user.user_id, build_user_id("alice", &server_name));

        assert!(store.check_password("alice", "correct horse").unwrap().is_some());
        assert!(store.check_password("alice", "wrong").unwrap().is_none());
        assert!(store.check_password("nobody", "correct horse").unwrap().is_none());
    }

    #[test]
    fn the_database_never_holds_the_plaintext_password() {
        let (store, server_name, path) = temp_store();
        store
            .register_user("alice", "correct horse", &server_name)
            .unwrap();

        let conn = rusqlite::Connection::open(&path).unwrap();
        let hash: String = conn
            .query_row(
                "SELECT password_hash FROM users WHERE localpart = 'alice'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(hash.starts_with("$argon2"));
        assert!(!hash.contains("correct horse"));
    }

    #[test]
    fn the_database_never_holds_a_raw_token() {
        let (store, server_name, path) = temp_store();
        let user = store
            .register_user("alice", "long enough", &server_name)
            .unwrap();
        let (_, token) = store.create_device(&user.user_id, "laptop").unwrap();

        let conn = rusqlite::Connection::open(&path).unwrap();
        let stored: String = conn
            .query_row("SELECT token_hash FROM devices", [], |row| row.get(0))
            .unwrap();

        assert_ne!(stored, token);
        assert!(!stored.contains(&token));
    }

    #[test]
    fn weak_credentials_are_rejected() {
        let (store, server_name, _path) = temp_store();
        assert!(store.register_user("alice", "short", &server_name).is_err());
        assert!(
            store
                .register_user("Alice Smith", "long enough", &server_name)
                .is_err()
        );
        assert!(store.register_user("alice", "long enough", &server_name).is_ok());
        assert!(store.register_user("alice", "long enough", &server_name).is_err());
    }

    #[test]
    fn tokens_identify_a_device_and_can_be_revoked() {
        let (store, server_name, _path) = temp_store();
        let user = store
            .register_user("alice", "long enough", &server_name)
            .unwrap();

        let (device, token) = store.create_device(&user.user_id, "laptop").unwrap();
        let found = store.device_for_token(&token).unwrap().unwrap();
        assert_eq!(found.device_id, device.device_id);
        assert_eq!(found.user_id, user.user_id);

        assert!(store.revoke_token(&token).unwrap());
        assert!(store.device_for_token(&token).unwrap().is_none());
    }

    #[test]
    fn one_account_can_hold_several_devices() {
        let (store, server_name, _path) = temp_store();
        let user = store
            .register_user("alice", "long enough", &server_name)
            .unwrap();

        let (laptop, laptop_token) = store.create_device(&user.user_id, "laptop").unwrap();
        let (phone, phone_token) = store.create_device(&user.user_id, "phone").unwrap();
        assert_ne!(laptop.device_id, phone.device_id);
        assert_ne!(laptop_token, phone_token);

        let laptop_key = SecretKey::from_bytes(&rand::random());
        let phone_key = SecretKey::from_bytes(&rand::random());
        store
            .bind_device_node(&laptop.device_id, laptop_key.public())
            .unwrap();
        store
            .bind_device_node(&phone.device_id, phone_key.public())
            .unwrap();

        assert_eq!(
            store.user_for_node(laptop_key.public()).unwrap().unwrap(),
            user.user_id
        );
        assert_eq!(
            store.user_for_node(phone_key.public()).unwrap().unwrap(),
            user.user_id
        );
    }

    #[test]
    fn room_creator_becomes_admin_and_membership_persists() {
        let (store, server_name, _path) = temp_store();
        let alice = store
            .register_user("alice", "long enough", &server_name)
            .unwrap();
        let bob = store
            .register_user("bob", "long enough", &server_name)
            .unwrap();

        let room = store.create_room("general", true, &alice.user_id).unwrap();
        assert_eq!(
            store.power_level(&room.id, &alice.user_id).unwrap(),
            POWER_ADMIN
        );
        assert!(store.is_joined(&room.id, &alice.user_id).unwrap());
        assert!(!store.is_joined(&room.id, &bob.user_id).unwrap());

        store
            .set_membership(
                &room.id,
                &bob.user_id,
                Membership::Invite,
                POWER_MEMBER,
                Some(&alice.user_id),
            )
            .unwrap();
        assert_eq!(
            store.membership(&room.id, &bob.user_id).unwrap().unwrap().0,
            Membership::Invite
        );
        assert_eq!(store.joined_count(&room.id).unwrap(), 1);

        let members = store.members(&room.id).unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].user_id, alice.user_id);
    }

    #[test]
    fn unknown_nodes_map_to_no_user() {
        let (store, _, _path) = temp_store();
        let stranger = SecretKey::from_bytes(&rand::random());
        assert!(store.user_for_node(stranger.public()).unwrap().is_none());
    }
}
