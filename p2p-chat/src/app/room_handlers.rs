use anyhow::Result;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};
use ticket::*;

use crate::app::{
    app_state::AppState,
    networking::{handle_messages, setup_networking},
    types,
    ui_handlers::update_online_users,
};

fn create_room_joined_message(ticket: &str) -> String {
    format!(
        "✅ Successfully joined room!\n\nYou can share this room token with others:\n\n[COPY TOKEN BELOW]\n{}\n[END TOKEN]\n\nInstructions: Select and copy the text between the brackets to share with others.",
        ticket
    )
}

fn create_room_created_message(ticket: &str) -> String {
    format!(
        "🎫 Room created successfully!\n\nShare this invitation token with others to join:\n\n[COPY TOKEN BELOW]\n{}\n[END TOKEN]\n\nInstructions: Select and copy the text between the brackets to share with others.",
        ticket
    )
}

pub async fn join_room(
    username: String,
    ticket_str: String,
    app_state: Arc<Mutex<AppState>>,
    chat_handle: Weak<types::ChatWindow>,
    join_handle: Weak<types::JoinWindow>,
) -> Result<()> {
    let ticket = Ticket::from_str(&ticket_str)?;
    let (sender, receiver, endpoint, router, _ticket) =
        setup_networking(Some(ticket), username.clone()).await?;

    {
        let mut state = app_state.lock().unwrap();
        state.sender = Some(sender);
        state.current_username = username.clone();
        state.current_node_id = Some(endpoint.node_id());
        state.endpoint = Some(endpoint.clone());
        state.router = Some(router);
    }

    let chat_handle_clone = chat_handle.clone();
    let app_state_clone = app_state.clone();
    tokio::spawn(async move {
        if let Err(e) = handle_messages(receiver, chat_handle_clone, app_state_clone).await {
            eprintln!("Error handling messages: {}", e);
        }
    });

    update_online_users(&chat_handle, &app_state);

    let chat_handle_for_ui = chat_handle.clone();
    let join_handle_for_ui = join_handle.clone();
    let username_for_ui = username.clone();
    let ticket_str_for_ui = ticket_str.clone();
    let app_state_for_ui = app_state.clone();

    match slint::invoke_from_event_loop(move || {
        if let Some(chat) = chat_handle_for_ui.upgrade() {
            chat.set_current_username(SharedString::from(username_for_ui.clone()));
            chat.set_connection_status(SharedString::from("Connected"));

            let ticket_message = create_room_joined_message(&ticket_str_for_ui);
            let system_message = types::ChatMessage {
                username: SharedString::from("System"),
                content: SharedString::from(ticket_message),
                timestamp: SharedString::from(chrono::Local::now().format("%H:%M").to_string()),
                is_own: false,
                is_system: true,
            };

            {
                let state = app_state_for_ui.lock().unwrap();
                let mut messages = state.messages.lock().unwrap();
                messages.push(system_message.clone());
            }

            let current_messages = {
                let state = app_state_for_ui.lock().unwrap();
                let messages = state.messages.lock().unwrap();
                messages.clone()
            };
            let messages_model = VecModel::from(current_messages);
            chat.set_messages(ModelRc::new(messages_model));

            if let Some(join) = join_handle_for_ui.upgrade() {
                let _ = chat.show();
                let _ = join.hide();
            }
        }
    }) {
        Ok(_) => {}
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to update UI: {:?}", e));
        }
    }

    Ok(())
}

pub async fn create_room(
    username: String,
    app_state: Arc<Mutex<AppState>>,
    chat_handle: Weak<types::ChatWindow>,
    create_handle: Weak<types::CreateWindow>,
) -> Result<()> {
    println!("Creating room for username: {}", username);
    let (sender, receiver, endpoint, router, room_ticket) =
        setup_networking(None, username.clone()).await?;
    println!("DEBUG: setup_networking returned successfully");
    println!("Networking setup complete");

    {
        let mut state = app_state.lock().unwrap();
        state.sender = Some(sender);
        state.current_username = username.clone();
        state.current_node_id = Some(endpoint.node_id());
        state.endpoint = Some(endpoint.clone());
        state.router = Some(router);
    }
    println!("App state updated");

    let chat_handle_clone = chat_handle.clone();
    let app_state_clone = app_state.clone();
    tokio::spawn(async move {
        if let Err(e) = handle_messages(receiver, chat_handle_clone, app_state_clone).await {
            eprintln!("Error handling messages: {}", e);
        }
    });
    println!("Message handler started");

    update_online_users(&chat_handle, &app_state);

    let chat_handle_for_ui = chat_handle.clone();
    let create_handle_for_ui = create_handle.clone();
    let username_for_ui = username.clone();
    let room_ticket_for_ui = room_ticket.to_string();
    let app_state_for_ui = app_state.clone();

    match slint::invoke_from_event_loop(move || {
        println!("Attempting to switch to chat window...");
        if let Some(chat) = chat_handle_for_ui.upgrade() {
            println!("Chat window found, updating UI");
            chat.set_current_username(SharedString::from(username_for_ui.clone()));
            chat.set_connection_status(SharedString::from("Connected"));

            let ticket_message = create_room_created_message(&room_ticket_for_ui);
            let system_message = types::ChatMessage {
                username: SharedString::from("System"),
                content: SharedString::from(ticket_message),
                timestamp: SharedString::from(chrono::Local::now().format("%H:%M").to_string()),
                is_own: false,
                is_system: true,
            };

            {
                let state = app_state_for_ui.lock().unwrap();
                let mut messages = state.messages.lock().unwrap();
                messages.push(system_message.clone());
            }

            let current_messages = {
                let state = app_state_for_ui.lock().unwrap();
                let messages = state.messages.lock().unwrap();
                messages.clone()
            };
            let messages_model = VecModel::from(current_messages);
            chat.set_messages(ModelRc::new(messages_model));

            println!("Added system message with room token");

            if let Some(create) = create_handle_for_ui.upgrade() {
                println!("Create window found, switching to chat");
                match chat.show() {
                    Ok(_) => println!("Chat window shown successfully"),
                    Err(e) => println!("Error showing chat window: {:?}", e),
                }
                match create.hide() {
                    Ok(_) => println!("Create window hidden successfully"),
                    Err(e) => println!("Error hiding create window: {:?}", e),
                }
                println!("Window switch completed");
            } else {
                println!("ERROR: Create window handle is invalid");
            }
        } else {
            println!("ERROR: Chat window handle is invalid");
        }
    }) {
        Ok(_) => println!("Event loop invocation successful"),
        Err(e) => {
            println!("ERROR: Failed to invoke on event loop: {:?}", e);
            return Err(anyhow::anyhow!("Failed to update UI: {:?}", e));
        }
    }

    println!("Create room completed successfully for: {}", username);
    Ok(())
}
