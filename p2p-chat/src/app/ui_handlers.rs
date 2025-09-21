use slint::{ModelRc, SharedString, VecModel, Weak};
use std::sync::{Arc, Mutex};

use crate::app::{app_state::AppState, types};

pub fn update_messages_and_clear_input(
    chat_handle: &Weak<types::ChatWindow>,
    app_state: &Arc<Mutex<AppState>>,
) {
    let chat_handle_clone = chat_handle.clone();
    let app_state_clone = app_state.clone();

    match slint::invoke_from_event_loop(move || {
        if let Some(chat) = chat_handle_clone.upgrade() {
            let state = app_state_clone.lock().unwrap();
            let messages = state.messages.lock().unwrap();

            println!(
                "DEBUG: Updating GUI with {} messages and clearing input",
                messages.len()
            );

            // Update messages first
            let messages_model = VecModel::from(messages.clone());
            chat.set_messages(ModelRc::new(messages_model));

            // Then clear the input
            chat.set_current_message("".into());

            println!("DEBUG: Messages updated and input cleared in GUI successfully");
        } else {
            println!("DEBUG: Chat window handle is invalid, cannot update messages or clear input");
        }
    }) {
        Ok(_) => {}
        Err(e) => println!("ERROR: Failed to invoke UI update from event loop: {:?}", e),
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
