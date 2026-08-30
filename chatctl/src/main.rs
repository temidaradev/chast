use anyhow::{Context, Result, bail};
use client::{AuthClient, Credentials, Net, RoomHistory};
use futures_lite::StreamExt;
use iroh_gossip::api::Event as GossipEvent;
use messaging::{
    MessageBody,
    rpc::{Request, Response, RoomSummary},
    user::power_label,
};
use std::str::FromStr;
use ticket::Ticket;

const JOIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str);

    match command {
        Some("connect") => connect(&args).await,
        Some("register") => register(&args).await,
        Some("login") => login(&args).await,
        Some("logout") => logout().await,
        Some("whoami") => whoami().await,
        Some("name") => set_display_name(&args).await,
        Some("rooms") => rooms().await,
        Some("create-room") => create_room(&args).await,
        Some("members") => members(&args).await,
        Some("join") => simple_room(&args, "join").await,
        Some("leave") => simple_room(&args, "leave").await,
        Some("invite") => target_room(&args, "invite").await,
        Some("kick") => target_room(&args, "kick").await,
        Some("ban") => target_room(&args, "ban").await,
        Some("unban") => target_room(&args, "unban").await,
        Some("power") => power(&args).await,
        Some("send") => send(&args).await,
        Some("read") => read(&args).await,
        Some("help") | None => {
            usage();
            Ok(())
        }
        Some(other) => {
            eprintln!("unknown command: {}", other);
            usage();
            std::process::exit(2);
        }
    }
}

fn usage() {
    println!("usage:");
    println!("  chatctl connect <address>             remember a server address or ticket");
    println!("  chatctl register <user> <password>    create an account");
    println!("  chatctl login <user> <password>       log in on this device");
    println!("  chatctl logout                        end this session");
    println!("  chatctl whoami                        show the current account");
    println!("  chatctl name <display name>           change your display name");
    println!("  chatctl rooms                         list rooms you can see");
    println!("  chatctl create-room <name> [--open]   create a room you administer");
    println!("  chatctl members <room>                list members and power levels");
    println!("  chatctl join <room>                   join a room");
    println!("  chatctl leave <room>                  leave a room");
    println!("  chatctl invite <room> <user-id>       invite someone");
    println!("  chatctl kick <room> <user-id>         remove someone");
    println!("  chatctl ban <room> <user-id>          ban someone");
    println!("  chatctl unban <room> <user-id>        lift a ban");
    println!("  chatctl power <room> <user-id> <n>    set a power level");
    println!("  chatctl send <room> <text>            post a message");
    println!("  chatctl read <room>                   sync and print the room history");
}

async fn client() -> Result<(Net, AuthClient, Credentials)> {
    let credentials = Credentials::load()?;
    let server = credentials.server()?;
    let net = Net::start().await?;
    let auth = AuthClient::from_endpoint(net.endpoint.clone(), server);
    Ok((net, auth, credentials))
}

async fn connect(args: &[String]) -> Result<()> {
    let ticket = args.get(1).context("usage: chatctl connect <server-address>")?;
    let mut credentials = Credentials::load()?;
    credentials.remember_server(ticket)?;
    credentials.save()?;

    println!("Server remembered: {}", credentials.server()?);
    println!("Now run: chatctl register <user> <password>");
    Ok(())
}

async fn register(args: &[String]) -> Result<()> {
    let user = args
        .get(1)
        .context("usage: chatctl register <user> <password>")?;
    let password = args
        .get(2)
        .context("usage: chatctl register <user> <password>")?;

    let (_net, auth, mut credentials) = client().await?;
    let response = auth
        .call_ok(Request::Register {
            localpart: user.clone(),
            password: password.clone(),
            device_name: device_name(),
        })
        .await?;

    store_session(&mut credentials, response)?;
    Ok(())
}

async fn login(args: &[String]) -> Result<()> {
    let user = args.get(1).context("usage: chatctl login <user> <password>")?;
    let password = args
        .get(2)
        .context("usage: chatctl login <user> <password>")?;

    let (_net, auth, mut credentials) = client().await?;
    let response = auth
        .call_ok(Request::Login {
            localpart: user.clone(),
            password: password.clone(),
            device_name: device_name(),
        })
        .await?;

    store_session(&mut credentials, response)?;
    Ok(())
}

fn store_session(credentials: &mut Credentials, response: Response) -> Result<()> {
    let Response::Session(session) = response else {
        bail!("unexpected answer from the server");
    };

    credentials.user_id = Some(session.user_id.clone());
    credentials.device_id = Some(session.device_id.clone());
    credentials.display_name = Some(session.display_name.clone());
    credentials.token = Some(session.token);
    credentials.server_name = Some(session.server_name);
    credentials.save()?;

    println!("Signed in as {}", session.user_id);
    println!("Device {}", session.device_id);
    Ok(())
}

fn device_name() -> String {
    std::env::var("P2P_CHAT_DEVICE").unwrap_or_else(|_| {
        std::env::var("HOSTNAME").unwrap_or_else(|_| "terminal".to_string())
    })
}

async fn logout() -> Result<()> {
    let (_net, auth, mut credentials) = client().await?;
    let token = credentials.token()?.to_string();
    auth.call_ok(Request::Logout { token }).await?;

    credentials.clear_session();
    credentials.save()?;
    println!("Signed out");
    Ok(())
}

async fn whoami() -> Result<()> {
    let (_net, auth, credentials) = client().await?;
    let response = auth
        .call_ok(Request::Whoami {
            token: credentials.token()?.to_string(),
        })
        .await?;

    match response {
        Response::Identity(identity) => {
            println!("{}", identity.user_id);
            println!("display name  {}", identity.display_name);
            println!("device        {}", identity.device_id);
            println!("server        {}", identity.server_name);
        }
        other => bail!("unexpected answer: {:?}", other),
    }

    Ok(())
}

async fn set_display_name(args: &[String]) -> Result<()> {
    let name = args
        .get(1..)
        .filter(|rest| !rest.is_empty())
        .map(|rest| rest.join(" "))
        .context("usage: chatctl name <display name>")?;

    let (_net, auth, mut credentials) = client().await?;
    auth.call_ok(Request::SetDisplayName {
        token: credentials.token()?.to_string(),
        display_name: name.clone(),
    })
    .await?;

    credentials.display_name = Some(name.clone());
    credentials.save()?;
    println!("Display name is now {}", name);
    Ok(())
}

fn print_rooms(rooms: &[RoomSummary]) {
    if rooms.is_empty() {
        println!("No rooms yet");
        return;
    }

    for room in rooms {
        println!(
            "{:<20} {:<10} {:<10} {} member(s){}",
            room.name,
            room.membership.as_str(),
            power_label(room.power_level),
            room.member_count,
            if room.invite_only {
                ", invite only"
            } else {
                ""
            }
        );
    }
}

async fn rooms() -> Result<()> {
    let (_net, auth, credentials) = client().await?;
    let response = auth
        .call_ok(Request::ListRooms {
            token: credentials.token()?.to_string(),
        })
        .await?;

    match response {
        Response::Rooms(rooms) => print_rooms(&rooms),
        other => bail!("unexpected answer: {:?}", other),
    }

    Ok(())
}

async fn create_room(args: &[String]) -> Result<()> {
    let positional: Vec<&String> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    let name = positional
        .first()
        .context("usage: chatctl create-room <name> [--open]")?;
    let invite_only = !args.iter().any(|a| a == "--open");

    let (_net, auth, credentials) = client().await?;
    let response = auth
        .call_ok(Request::CreateRoom {
            token: credentials.token()?.to_string(),
            name: (*name).clone(),
            invite_only,
        })
        .await?;

    match response {
        Response::Room(room) => {
            println!("Created \"{}\" ({})", room.name, room.id);
            println!("You are {}", power_label(room.power_level));
            println!("{}", room.ticket);
        }
        other => bail!("unexpected answer: {:?}", other),
    }

    Ok(())
}

async fn members(args: &[String]) -> Result<()> {
    let room = args.get(1).context("usage: chatctl members <room>")?;

    let (_net, auth, credentials) = client().await?;
    let response = auth
        .call_ok(Request::RoomMembers {
            token: credentials.token()?.to_string(),
            room: room.clone(),
        })
        .await?;

    match response {
        Response::Members(members) => {
            for member in members {
                println!(
                    "{:<28} {:<8} {:<10} {}",
                    member.user_id,
                    member.membership.as_str(),
                    power_label(member.power_level),
                    member.display_name
                );
            }
        }
        other => bail!("unexpected answer: {:?}", other),
    }

    Ok(())
}

async fn simple_room(args: &[String], action: &str) -> Result<()> {
    let room = args
        .get(1)
        .with_context(|| format!("usage: chatctl {} <room>", action))?;

    let (_net, auth, credentials) = client().await?;
    let token = credentials.token()?.to_string();

    let request = match action {
        "join" => Request::JoinRoom {
            token,
            room: room.clone(),
        },
        _ => Request::LeaveRoom {
            token,
            room: room.clone(),
        },
    };

    match auth.call_ok(request).await? {
        Response::Room(room) => println!(
            "Joined \"{}\" as {}",
            room.name,
            power_label(room.power_level)
        ),
        _ => println!("Left \"{}\"", room),
    }

    Ok(())
}

async fn target_room(args: &[String], action: &str) -> Result<()> {
    let room = args
        .get(1)
        .with_context(|| format!("usage: chatctl {} <room> <user-id>", action))?;
    let user_id = args
        .get(2)
        .with_context(|| format!("usage: chatctl {} <room> <user-id>", action))?;

    let (_net, auth, credentials) = client().await?;
    let token = credentials.token()?.to_string();
    let room = room.clone();
    let user_id = user_id.clone();

    let request = match action {
        "invite" => Request::Invite {
            token,
            room: room.clone(),
            user_id: user_id.clone(),
        },
        "kick" => Request::Kick {
            token,
            room: room.clone(),
            user_id: user_id.clone(),
        },
        "ban" => Request::Ban {
            token,
            room: room.clone(),
            user_id: user_id.clone(),
        },
        _ => Request::Unban {
            token,
            room: room.clone(),
            user_id: user_id.clone(),
        },
    };

    auth.call_ok(request).await?;
    println!("{} {} in \"{}\"", action, user_id, room);
    Ok(())
}

async fn power(args: &[String]) -> Result<()> {
    let room = args
        .get(1)
        .context("usage: chatctl power <room> <user-id> <level>")?;
    let user_id = args
        .get(2)
        .context("usage: chatctl power <room> <user-id> <level>")?;
    let level: i64 = args
        .get(3)
        .context("usage: chatctl power <room> <user-id> <level>")?
        .parse()
        .context("level must be a number, for example 100 or 50")?;

    let (_net, auth, credentials) = client().await?;
    auth.call_ok(Request::SetPowerLevel {
        token: credentials.token()?.to_string(),
        room: room.clone(),
        user_id: user_id.clone(),
        level,
    })
    .await?;

    println!("{} is now {} in \"{}\"", user_id, power_label(level), room);
    Ok(())
}

async fn room_ticket(auth: &AuthClient, credentials: &Credentials, room: &str) -> Result<Ticket> {
    let response = auth
        .call_ok(Request::ListRooms {
            token: credentials.token()?.to_string(),
        })
        .await?;

    let Response::Rooms(rooms) = response else {
        bail!("unexpected answer from the server");
    };

    let found = rooms
        .into_iter()
        .find(|summary| summary.name == room || summary.id == room)
        .with_context(|| format!("you have no room called {}", room))?;

    Ticket::from_str(&found.ticket).context("the server sent an invalid ticket")
}

struct Joined {
    sender: iroh_gossip::api::GossipSender,
    receiver: iroh_gossip::api::GossipReceiver,
    topic: iroh_gossip::proto::TopicId,
    secret_key: iroh::SecretKey,
    node_id: iroh::NodeId,
}

async fn join_gossip(net: &Net, ticket: Ticket) -> Result<Joined> {
    let secret_key = net.secret_key.clone();
    let node_ids: Vec<iroh::NodeId> = ticket.nodes.iter().map(|n| n.node_id).collect();
    for node in ticket.nodes {
        net.endpoint.add_node_addr(node)?;
    }

    let joining = net.gossip.subscribe_and_join(ticket.topic, node_ids);
    let (sender, receiver) = match tokio::time::timeout(JOIN_TIMEOUT, joining).await {
        Ok(joined) => joined?.split(),
        Err(_) => bail!("nobody answered in this room, the server may be offline"),
    };

    sender
        .broadcast(messaging::sign(&secret_key, MessageBody::Hello)?.into())
        .await?;

    Ok(Joined {
        sender,
        receiver,
        topic: ticket.topic,
        secret_key,
        node_id: net.endpoint.node_id(),
    })
}

async fn send(args: &[String]) -> Result<()> {
    let room = args.get(1).context("usage: chatctl send <room> <text>")?;
    let text = args
        .get(2..)
        .filter(|rest| !rest.is_empty())
        .map(|rest| rest.join(" "))
        .context("usage: chatctl send <room> <text>")?;

    let (net, auth, credentials) = client().await?;
    let ticket = room_ticket(&auth, &credentials, room).await?;

    let mut joined = join_gossip(&net, ticket).await?;
    let node_id = joined.node_id;
    let id = messaging::new_event_id();

    tokio::time::sleep(tokio::time::Duration::from_millis(1200)).await;

    joined
        .sender
        .broadcast(
            messaging::sign(
                &joined.secret_key,
                MessageBody::Message {
                    id: id.clone(),
                    text: text.clone(),
                },
            )?
            .into(),
        )
        .await?;

    let refusal = wait_for_refusal(&mut joined, node_id).await;


    match refusal {
        Some(reason) => bail!("the server refused: {}", reason),
        None => {
            println!("Sent to \"{}\": {}", room, text);
            Ok(())
        }
    }
}

async fn wait_for_refusal(joined: &mut Joined, node_id: iroh::NodeId) -> Option<String> {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(2500);

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }

        let Ok(Ok(Some(event))) =
            tokio::time::timeout(remaining, joined.receiver.try_next()).await
        else {
            return None;
        };

        let GossipEvent::Received(msg) = event else {
            continue;
        };
        let Ok((_, body)) = messaging::verify(&msg.content) else {
            continue;
        };

        if let MessageBody::Rejected { to, reason } = body {
            if to == node_id {
                return Some(reason);
            }
        }
    }
}

async fn read(args: &[String]) -> Result<()> {
    let room = args.get(1).context("usage: chatctl read <room>")?;

    let (net, auth, credentials) = client().await?;
    let ticket = room_ticket(&auth, &credentials, room).await?;

    let mut joined = join_gossip(&net, ticket).await?;
    let mut history = RoomHistory::load(&joined.topic, "");
    let node_id = joined.node_id;

    tokio::time::sleep(tokio::time::Duration::from_millis(1200)).await;
    joined
        .sender
        .broadcast(
            messaging::sign(
                &joined.secret_key,
                MessageBody::SyncRequest {
                    since_seq: history.last_seq,
                },
            )?
            .into(),
        )
        .await?;

    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(15);
    let mut printed = 0;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }

        let Ok(Ok(Some(event))) =
            tokio::time::timeout(remaining, joined.receiver.try_next()).await
        else {
            break;
        };

        let GossipEvent::Received(msg) = event else {
            continue;
        };

        let Ok((_, body)) = messaging::verify(&msg.content) else {
            continue;
        };

        match body {
            MessageBody::SyncResponse {
                to,
                events,
                latest_seq,
            } if to == node_id => {
                let mut highest = 0;
                for event in events {
                    highest = highest.max(event.seq);
                    if history.record(event.clone()) {
                        println!(
                            "#{:<4} {:<20} {}",
                            event.seq, event.sender_name, event.text
                        );
                        printed += 1;
                    }
                }
                history.advance(highest);
                history.save(&joined.topic).ok();

                if highest == 0 || highest >= latest_seq {
                    break;
                }

                joined
                    .sender
                    .broadcast(
                        messaging::sign(
                            &joined.secret_key,
                            MessageBody::SyncRequest {
                                since_seq: highest,
                            },
                        )?
                        .into(),
                    )
                    .await?;
            }
            MessageBody::Rejected { to, reason } if to == node_id => {
                bail!("the server refused: {}", reason);
            }
            _ => {}
        }
    }

    if printed == 0 {
        println!("Nothing new in \"{}\"", room);
    }

    Ok(())
}
