use anyhow::Result;
use messaging::{rpc::RoomSummary, user::power_label};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use std::sync::{Arc, Mutex};

use crate::app::{
    app_state::{AppState, RosterEntry},
    networking::{Mode, event_to_chat_message, handle_messages, join_gossip, request_sync},
    session::Session,
    types,
    ui_handlers::{push_system, update_members, update_messages},
};
use client::{Net, RoomHistory};
use std::str::FromStr;
use ticket::Ticket;

pub fn room_items(rooms: &[RoomSummary]) -> Vec<types::RoomItem> {
    rooms
        .iter()
        .map(|room| types::RoomItem {
            id: SharedString::from(room.id.clone()),
            name: SharedString::from(room.name.clone()),
            membership: SharedString::from(room.membership.as_str()),
            role: SharedString::from(power_label(room.power_level)),
            members: room.member_count as i32,
            invite_only: room.invite_only,
        })
        .collect()
}

pub async fn refresh_rooms(
    session: &Session,
    rooms_handle: Weak<types::RoomsWindow>,
) -> Result<()> {
    let rooms = session.rooms().await?;
    let items = room_items(&rooms);
    let user_id = session.user_id();
    let display_name = session.display_name();

    slint::invoke_from_event_loop(move || {
        if let Some(window) = rooms_handle.upgrade() {
            window.set_rooms(ModelRc::new(VecModel::from(items)));
            window.set_user_id(SharedString::from(user_id));
            window.set_display_name(SharedString::from(display_name));
            window.set_status(SharedString::from(""));
        }
    })
    .map_err(|e| anyhow::anyhow!("failed to update the room list: {:?}", e))?;

    Ok(())
}

pub async fn open_room(
    net: &Net,
    session: Arc<Session>,
    room_name: String,
    app_state: Arc<Mutex<AppState>>,
    chat_handle: Weak<types::ChatWindow>,
    rooms_handle: Weak<types::RoomsWindow>,
) -> Result<()> {
    let (summary, ticket) = session.open(&room_name).await?;
    let server_node_id = ticket.nodes.first().map(|node| node.node_id);

    let connected = join_gossip(net, Some(ticket), Mode::Server).await?;
    let node_id = connected.node_id;

    let my_user = session.user_id();
    let history = RoomHistory::load(&connected.topic, &summary.ticket);
    let restored: Vec<types::ChatMessage> = history
        .events
        .iter()
        .map(|event| event_to_chat_message(event, &my_user))
        .collect();

    let members = session.members(&summary.name).await.unwrap_or_default();

    {
        let mut state = app_state.lock().unwrap();
        state.sender = Some(connected.sender);
        state.secret_key = Some(connected.secret_key);
        state.current_node_id = Some(node_id);
        state.server_node_id = server_node_id;
        state.current_topic = Some(connected.topic);
        state.current_session_token = Some(summary.ticket.clone());
        state.current_room = summary.name.clone();
        state.my_user_id = my_user.clone();
        state.display_name = session.display_name();
        state.my_power = summary.power_level;
        *state.history.lock().unwrap() = history;
        *state.messages.lock().unwrap() = restored;

        let mut roster = state.roster.lock().unwrap();
        roster.clear();
        for member in members {
            if member.membership == messaging::Membership::Join {
                roster.insert(
                    member.user_id.clone(),
                    RosterEntry {
                        user_id: member.user_id,
                        display_name: member.display_name,
                        power_level: member.power_level,
                        membership: member.membership,
                    },
                );
            }
        }
    }

    let chat_for_messages = chat_handle.clone();
    let state_for_messages = app_state.clone();
    tokio::spawn(async move {
        if let Err(e) =
            handle_messages(connected.receiver, chat_for_messages, state_for_messages).await
        {
            eprintln!("error handling messages: {}", e);
        }
    });

    let room_label = summary.name.clone();
    let role = power_label(summary.power_level).to_string();
    let chat_for_ui = chat_handle.clone();
    let state_for_ui = app_state.clone();

    slint::invoke_from_event_loop(move || {
        if let Some(chat) = chat_for_ui.upgrade() {
            chat.set_room_name(SharedString::from(room_label));
            chat.set_my_role(SharedString::from(role));
            chat.set_current_username(SharedString::from(
                state_for_ui.lock().unwrap().display_name.clone(),
            ));
            chat.set_connection_status(SharedString::from("Connected"));

            let messages = {
                let state = state_for_ui.lock().unwrap();
                let messages = state.messages.lock().unwrap();
                messages.clone()
            };
            chat.set_messages(ModelRc::new(VecModel::from(messages)));
            let _ = chat.show();
        }

        if let Some(rooms) = rooms_handle.upgrade() {
            let _ = rooms.hide();
        }
    })
    .map_err(|e| anyhow::anyhow!("failed to show the chat window: {:?}", e))?;

    update_members(&chat_handle, &app_state);
    update_messages(&chat_handle, &app_state);

    tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
    request_sync(&app_state).await?;

    Ok(())
}

pub async fn open_peer_room(
    net: &Net,
    ticket_str: String,
    display_name: String,
    app_state: Arc<Mutex<AppState>>,
    chat_handle: Weak<types::ChatWindow>,
    auth_handle: Weak<types::AuthWindow>,
) -> Result<()> {
    let ticket = match ticket_str.trim() {
        "" => None,
        raw => Some(Ticket::from_str(raw).map_err(|_| anyhow::anyhow!("that room ticket is not valid"))?),
    };

    let joining = ticket.is_some();
    let connected = join_gossip(
        net,
        ticket,
        Mode::Peer {
            display_name: display_name.clone(),
        },
    )
    .await?;

    let node_id = connected.node_id;
    let my_user = node_id.to_string();
    let share = connected.ticket.to_string();

    let history = RoomHistory::load(&connected.topic, &share);
    let restored: Vec<types::ChatMessage> = history
        .events
        .iter()
        .map(|event| event_to_chat_message(event, &my_user))
        .collect();

    {
        let mut state = app_state.lock().unwrap();
        state.sender = Some(connected.sender);
        state.secret_key = Some(connected.secret_key);
        state.current_node_id = Some(node_id);
        state.server_node_id = None;
        state.current_topic = Some(connected.topic);
        state.current_session_token = Some(share);
        state.current_room = "Direct room".to_string();
        state.my_user_id = my_user.clone();
        state.display_name = display_name.clone();
        state.my_power = 0;
        *state.history.lock().unwrap() = history;
        *state.messages.lock().unwrap() = restored;

        let mut roster = state.roster.lock().unwrap();
        roster.clear();
        roster.insert(
            my_user.clone(),
            RosterEntry {
                user_id: my_user.clone(),
                display_name: display_name.clone(),
                power_level: 0,
                membership: messaging::Membership::Join,
            },
        );
        state.nodes.lock().unwrap().insert(node_id, my_user);
    }

    push_system(
        &app_state,
        if joining {
            "Joined a direct peer to peer room. There is no server here, so nothing is stored for you and names are self declared.".to_string()
        } else {
            "Created a direct peer to peer room. Share the ticket with the Copy Token button. It only works while you stay open.".to_string()
        },
    );

    let chat_for_messages = chat_handle.clone();
    let state_for_messages = app_state.clone();
    tokio::spawn(async move {
        if let Err(e) =
            handle_messages(connected.receiver, chat_for_messages, state_for_messages).await
        {
            eprintln!("error handling messages: {}", e);
        }
    });

    let chat_for_ui = chat_handle.clone();
    let state_for_ui = app_state.clone();

    slint::invoke_from_event_loop(move || {
        if let Some(chat) = chat_for_ui.upgrade() {
            chat.set_room_name(SharedString::from("Direct room"));
            chat.set_my_role(SharedString::from("peer to peer, no account"));
            chat.set_current_username(SharedString::from(display_name));
            chat.set_connection_status(SharedString::from("Connected"));

            let messages = {
                let state = state_for_ui.lock().unwrap();
                let messages = state.messages.lock().unwrap();
                messages.clone()
            };
            chat.set_messages(ModelRc::new(VecModel::from(messages)));
            let _ = chat.show();
        }

        if let Some(auth) = auth_handle.upgrade() {
            auth.set_busy(false);
            let _ = auth.hide();
        }
    })
    .map_err(|e| anyhow::anyhow!("failed to show the chat window: {:?}", e))?;

    update_members(&chat_handle, &app_state);
    update_messages(&chat_handle, &app_state);

    Ok(())
}
