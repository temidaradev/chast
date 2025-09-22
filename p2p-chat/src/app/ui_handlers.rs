use slint::{ModelRc, SharedString, VecModel, Weak};
use std::sync::{Arc, Mutex};

use crate::app::{app_state::AppState, types};

pub fn handle_user_connect(
    chat_handle: &Weak<types::ChatWindow>,
    app_state: &Arc<Mutex<AppState>>,
    username: &str,
) {
    let connect_message = types::ChatMessage {
        username: SharedString::from("System"),
        content: SharedString::from(format!("🎉 {} joined the room", username)),
        timestamp: SharedString::from(chrono::Local::now().format("%H:%M").to_string()),
        is_own: false,
        is_system: true,
    };

    {
        let state = app_state.lock().unwrap();
        let mut messages = state.messages.lock().unwrap();
        messages.push(connect_message);
    }

    update_messages(chat_handle, app_state);
    update_online_users(chat_handle, app_state);
    
    println!("DEBUG: User {} connected, UI updated", username);
}

pub fn handle_user_disconnect(
    chat_handle: &Weak<types::ChatWindow>,
    app_state: &Arc<Mutex<AppState>>,
    username: &str,
) {
    let disconnect_message = types::ChatMessage {
        username: SharedString::from("System"),
        content: SharedString::from(format!("👋 {} disconnected", username)),
        timestamp: SharedString::from(chrono::Local::now().format("%H:%M").to_string()),
        is_own: false,
        is_system: true,
    };

    {
        let state = app_state.lock().unwrap();
        let mut messages = state.messages.lock().unwrap();
        messages.push(disconnect_message);
    }

    update_messages(chat_handle, app_state);
    update_online_users(chat_handle, app_state);
    
    println!("DEBUG: User {} disconnected, UI updated", username);
}

pub fn update_messages_and_clear_input(
    chat_handle: &Weak<types::ChatWindow>,
    app_state: &Arc<Mutex<AppState>>,
) {
    update_messages(chat_handle, app_state);

    let chat_handle_clone = chat_handle.clone();

    match slint::invoke_from_event_loop(move || {
        if let Some(chat) = chat_handle_clone.upgrade() {
            chat.set_current_message("".into());
            println!("DEBUG: Input cleared in GUI successfully");
        } else {
            println!("DEBUG: Chat window handle is invalid, cannot clear input");
        }
    }) {
        Ok(_) => {}
        Err(e) => println!("ERROR: Failed to clear input from event loop: {:?}", e),
    }
}

pub fn update_online_users(
    chat_handle: &Weak<types::ChatWindow>,
    app_state: &Arc<Mutex<AppState>>,
) {
    let chat_handle_clone = chat_handle.clone();
    let app_state_clone = app_state.clone();

    match slint::invoke_from_event_loop(move || {
        if let Some(chat) = chat_handle_clone.upgrade() {
            let state = app_state_clone.lock().unwrap();
            let names = state.names.lock().unwrap();

            let mut users: Vec<SharedString> = names
                .values()
                .map(|name| SharedString::from(name.clone()))
                .collect();
            users.push(SharedString::from(state.current_username.clone()));
            users.sort();
            users.dedup();

            let users_model = VecModel::from(users);
            chat.set_online_users(ModelRc::new(users_model));
            println!("DEBUG: Online users updated in GUI");
        } else {
            println!("DEBUG: Chat window handle is invalid, cannot update online users");
        }
    }) {
        Ok(_) => {}
        Err(e) => println!(
            "ERROR: Failed to invoke online users update from event loop: {:?}",
            e
        ),
    }
}

pub fn update_messages(chat_handle: &Weak<types::ChatWindow>, app_state: &Arc<Mutex<AppState>>) {
    let chat_handle_clone = chat_handle.clone();
    let app_state_clone = app_state.clone();

    match slint::invoke_from_event_loop(move || {
        if let Some(chat) = chat_handle_clone.upgrade() {
            let state = app_state_clone.lock().unwrap();
            let messages = state.messages.lock().unwrap();

            println!("DEBUG: Updating GUI with {} messages", messages.len());
            let messages_model = VecModel::from(messages.clone());
            chat.set_messages(ModelRc::new(messages_model));
            println!("DEBUG: Messages updated in GUI successfully");
        } else {
            println!("DEBUG: Chat window handle is invalid, cannot update messages");
        }
    }) {
        Ok(_) => {}
        Err(e) => println!("ERROR: Failed to invoke UI update from event loop: {:?}", e),
    }
}
