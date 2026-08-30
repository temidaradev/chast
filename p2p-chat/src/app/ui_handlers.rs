use messaging::user::power_label;
use slint::{ModelRc, SharedString, VecModel, Weak};
use std::sync::{Arc, Mutex};

use crate::app::{app_state::AppState, types};

pub fn push_system(app_state: &Arc<Mutex<AppState>>, text: String) {
    let message = types::ChatMessage {
        username: SharedString::from("System"),
        content: SharedString::from(text),
        timestamp: SharedString::from(chrono::Local::now().format("%H:%M").to_string()),
        is_own: false,
        is_system: true,
    };

    let state = app_state.lock().unwrap();
    state.messages.lock().unwrap().push(message);
}

pub fn update_members(chat_handle: &Weak<types::ChatWindow>, app_state: &Arc<Mutex<AppState>>) {
    let chat_handle = chat_handle.clone();
    let app_state = app_state.clone();

    let result = slint::invoke_from_event_loop(move || {
        let Some(chat) = chat_handle.upgrade() else {
            return;
        };

        let state = app_state.lock().unwrap();
        let roster = state.roster.lock().unwrap();

        let mut members: Vec<types::MemberItem> = roster
            .values()
            .map(|entry| types::MemberItem {
                user_id: SharedString::from(entry.user_id.clone()),
                display_name: SharedString::from(entry.display_name.clone()),
                membership: SharedString::from(entry.membership.as_str()),
                role: SharedString::from(power_label(entry.power_level)),
                is_me: entry.user_id == state.my_user_id,
            })
            .collect();

        members.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        chat.set_members(ModelRc::new(VecModel::from(members)));
    });

    if let Err(e) = result {
        eprintln!("failed to update members: {:?}", e);
    }
}

pub fn update_messages(chat_handle: &Weak<types::ChatWindow>, app_state: &Arc<Mutex<AppState>>) {
    let chat_handle = chat_handle.clone();
    let app_state = app_state.clone();

    let result = slint::invoke_from_event_loop(move || {
        let Some(chat) = chat_handle.upgrade() else {
            return;
        };

        let state = app_state.lock().unwrap();
        let messages = state.messages.lock().unwrap();
        chat.set_messages(ModelRc::new(VecModel::from(messages.clone())));
    });

    if let Err(e) = result {
        eprintln!("failed to update messages: {:?}", e);
    }
}

pub fn update_messages_and_clear_input(
    chat_handle: &Weak<types::ChatWindow>,
    app_state: &Arc<Mutex<AppState>>,
) {
    update_messages(chat_handle, app_state);

    let chat_handle = chat_handle.clone();
    let result = slint::invoke_from_event_loop(move || {
        if let Some(chat) = chat_handle.upgrade() {
            chat.set_current_message("".into());
        }
    });

    if let Err(e) = result {
        eprintln!("failed to clear the input: {:?}", e);
    }
}
