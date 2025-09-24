use anyhow::Result;
use slint::{ComponentHandle, Weak, ModelRc, VecModel};
use std::{sync::{Arc, Mutex}, rc::Rc};
use chrono::Utc;

use crate::app::{
    app_state::AppState,
    networking::{send_message, send_disconnect},
    room_handlers::{create_room, join_room},
    types::{self, ChatFileItem},
    ui_handlers::update_messages,
    save::{ChatSaveManager, ChatSession, ChatMessage, MessageType, Config, ChatFileInfo},
};

pub struct App {}

fn chat_file_to_slint_item(file_info: &ChatFileInfo) -> ChatFileItem {
    ChatFileItem {
        file_path: file_info.path.to_string_lossy().to_string().into(),
        display_name: extract_display_name(&file_info.filename).into(),
        last_modified: file_info.last_modified.format("%Y-%m-%d %H:%M").to_string().into(),
        size_display: format_file_size(file_info.size).into(),
        selected: false,
    }
}

fn extract_display_name(filename: &str) -> String {
    if let Some(name) = filename.strip_prefix("chat_").and_then(|s| s.strip_suffix(".json")) {
        let parts: Vec<&str> = name.split('_').collect();
        if parts.len() >= 3 {
            let session_id = parts[0];
            let date = parts[1];
            let time = parts[2];
            return format!("Session {} - {}/{}/{} {}:{}:{}", 
                session_id,
                &date[0..4], &date[4..6], &date[6..8],
                &time[0..2], &time[2..4], &time[4..6]
            );
        }
    }
    filename.to_string()
}

fn format_file_size(size: u64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    }
}

fn app_state_to_chat_session(app_state: &Arc<Mutex<AppState>>, session_id: String) -> ChatSession {
    let state = app_state.lock().unwrap();
    let messages = state.messages.lock().unwrap();
    let names = state.names.lock().unwrap();
    
    let chat_messages: Vec<ChatMessage> = messages.iter().map(|msg| {
        ChatMessage {
            sender: msg.username.to_string(),
            content: msg.content.to_string(),
            timestamp: Utc::now(),
            message_type: if msg.is_system { MessageType::System } else { MessageType::Text },
        }
    }).collect();
    
    let participants: Vec<String> = names.values().cloned().collect();
    
    ChatSession {
        session_id,
        participants,
        messages: chat_messages,
        created_at: Utc::now(), 
        last_updated: Utc::now(),
    }
}

impl App {
    pub fn run() -> Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;

        let main = types::StartWindow::new()?;
        let main_handle = main.as_weak();

        let join = types::JoinWindow::new()?;
        let join_handle = join.as_weak();

        let create = types::CreateWindow::new()?;
        let create_handle = create.as_weak();

        let chat = types::ChatWindow::new()?;
        let chat_handle = chat.as_weak();

        let app_state = Arc::new(Mutex::new(AppState::new()));

        let config = Config::default();
        let save_manager = Arc::new(Mutex::new(ChatSaveManager::new(config)?));

        Self::setup_navigation(&main_handle, &join_handle, &create_handle, &chat_handle);

        Self::setup_networking_callbacks(
            &main_handle,
            &join_handle,
            &create_handle,
            &chat_handle,
            app_state.clone(),
            rt.handle().clone(),
            save_manager.clone(),
        );

        Self::setup_save_callbacks(
            &chat_handle,
            app_state.clone(),
            save_manager.clone(),
        );

        let _ = main.show();

        let result = main.run();

        rt.shutdown_timeout(std::time::Duration::from_secs(5));

        result.map_err(|e| anyhow::anyhow!("Slint error: {}", e))
    }

    fn setup_navigation(
        main_handle: &Weak<types::StartWindow>,
        join_handle: &Weak<types::JoinWindow>,
        create_handle: &Weak<types::CreateWindow>,
        _chat_handle: &Weak<types::ChatWindow>,
    ) {
        {
            let main_handle_clone = main_handle.clone();
            let join_handle_clone = join_handle.clone();
            if let Some(main) = main_handle.upgrade() {
                main.on_switch_to_join_window(move || {
                    if let (Some(main), Some(join)) =
                        (main_handle_clone.upgrade(), join_handle_clone.upgrade())
                    {
                        let _ = join.show();
                        let _ = main.hide();
                    }
                });
            }
        }

        {
            let main_handle_clone = main_handle.clone();
            let create_handle_clone = create_handle.clone();
            if let Some(main) = main_handle.upgrade() {
                main.on_switch_to_create_window(move || {
                    if let (Some(main), Some(create)) =
                        (main_handle_clone.upgrade(), create_handle_clone.upgrade())
                    {
                        let _ = create.show();
                        let _ = main.hide();
                    }
                });
            }
        }

        {
            let main_handle_clone = main_handle.clone();
            let join_handle_clone = join_handle.clone();
            if let Some(join) = join_handle.upgrade() {
                join.on_switch_to_start_window(move || {
                    if let (Some(main), Some(join)) =
                        (main_handle_clone.upgrade(), join_handle_clone.upgrade())
                    {
                        let _ = main.show();
                        let _ = join.hide();
                    }
                });
            }
        }

        {
            let main_handle_clone = main_handle.clone();
            let create_handle_clone = create_handle.clone();
            if let Some(create) = create_handle.upgrade() {
                create.on_switch_to_start_window(move || {
                    if let (Some(main), Some(create)) =
                        (main_handle_clone.upgrade(), create_handle_clone.upgrade())
                    {
                        let _ = main.show();
                        let _ = create.hide();
                    }
                });
            }
        }
    }

    fn setup_networking_callbacks(
        main_handle: &Weak<types::StartWindow>,
        join_handle: &Weak<types::JoinWindow>,
        create_handle: &Weak<types::CreateWindow>,
        chat_handle: &Weak<types::ChatWindow>,
        app_state: Arc<Mutex<AppState>>,
        rt_handle: tokio::runtime::Handle,
        save_manager: Arc<Mutex<ChatSaveManager>>,
    ) {
        {
            let app_state_clone = app_state.clone();
            let chat_handle_clone = chat_handle.clone();
            let join_handle_clone = join_handle.clone();
            let rt_handle_clone = rt_handle.clone();

            if let Some(join) = join_handle.upgrade() {
                join.on_switch_to_chat_window(move |username, ticket_str| {
                    let app_state = app_state_clone.clone();
                    let chat_handle = chat_handle_clone.clone();
                    let join_handle = join_handle_clone.clone();
                    let username = username.to_string();
                    let ticket_str = ticket_str.to_string();

                    rt_handle_clone.spawn(async move {
                        if let Err(e) =
                            join_room(username, ticket_str, app_state, chat_handle, join_handle)
                                .await
                        {
                            eprintln!("Error joining room: {}", e);
                        }
                    });
                });
            }
        }

        {
            let app_state_clone = app_state.clone();
            let chat_handle_clone = chat_handle.clone();
            let create_handle_clone = create_handle.clone();
            let rt_handle_clone = rt_handle.clone();

            if let Some(create) = create_handle.upgrade() {
                create.on_switch_to_chat_window(move |username| {
                    let app_state = app_state_clone.clone();
                    let chat_handle = chat_handle_clone.clone();
                    let create_handle = create_handle_clone.clone();
                    let username = username.to_string();

                    rt_handle_clone.spawn(async move {
                        if let Err(e) =
                            create_room(username, app_state, chat_handle, create_handle).await
                        {
                            eprintln!("Error creating room: {}", e);
                        }
                    });
                });
            }
        }

        {
            let app_state_clone = app_state.clone();
            let chat_handle_clone = chat_handle.clone();
            let rt_handle_clone = rt_handle.clone();
            let save_manager_clone = save_manager.clone();

            if let Some(chat) = chat_handle.upgrade() {
                chat.on_send_message(move |message| {
                    println!("DEBUG: Sending message: '{}'", message);
                    let app_state = app_state_clone.clone();
                    let chat_handle = chat_handle_clone.clone();
                    let message = message.to_string();
                    let save_manager = save_manager_clone.clone();

                    rt_handle_clone.spawn(async move {
                        match send_message(message.clone(), app_state.clone()).await {
                            Ok(_) => {
                                println!("DEBUG: Message sent successfully, updating UI");
                                update_messages(&chat_handle, &app_state);
                                
                                let session_id = format!("session_{}", 
                                    std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs()
                                );
                                let session = app_state_to_chat_session(&app_state, session_id);
                                if let Err(e) = save_manager.lock().unwrap().auto_save_chat(&session) {
                                    eprintln!("Failed to auto-save chat: {}", e);
                                }
                            }
                            Err(e) => {
                                eprintln!("ERROR: Failed to send message: {}", e);
                            }
                        }
                    });
                });
            }
        }

        {
            let app_state_clone = app_state.clone();
            let main_handle_clone = main_handle.clone();
            let chat_handle_clone = chat_handle.clone();
            let rt_handle_clone = rt_handle.clone();

            if let Some(chat) = chat_handle.upgrade() {
                chat.on_disconnect(move || {
                    let app_state = app_state_clone.clone();
                    let main_handle = main_handle_clone.clone();
                    let chat_handle = chat_handle_clone.clone();

                    rt_handle_clone.spawn(async move {
                        if let Err(e) = send_disconnect(app_state.clone()).await {
                            eprintln!("Error sending disconnect message: {}", e);
                        }

                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                        let router = {
                            let mut state = app_state.lock().unwrap();
                            let router = state.router.take();
                            state.sender = None;
                            state.endpoint = None;
                            state.current_node_id = None;
                            state.names.lock().unwrap().clear();
                            state.messages.lock().unwrap().clear();
                            router
                        };

                        if let Some(router) = router
                            && let Err(e) = router.shutdown().await {
                                eprintln!("Error shutting down router: {}", e);
                            }

                        match slint::invoke_from_event_loop(move || {
                            if let Some(chat) = chat_handle.upgrade() {
                                if let Some(main) = main_handle.upgrade() {
                                    match main.show() {
                                        Ok(_) => println!("Main window shown successfully"),
                                        Err(e) => println!("Error showing main window: {:?}", e),
                                    }
                                    match chat.hide() {
                                        Ok(_) => println!("Chat window hidden successfully"),
                                        Err(e) => println!("Error hiding chat window: {:?}", e),
                                    }
                                } else {
                                    println!("ERROR: Main window handle is invalid");
                                }
                            } else {
                                println!("ERROR: Chat window handle is invalid");
                            }
                        }) {
                            Ok(_) => println!("Disconnect navigation successful"),
                            Err(e) => {
                                println!("ERROR: Failed to invoke disconnect navigation: {:?}", e)
                            }
                        }
                    });
                });
            }
        }
    }

    fn setup_save_callbacks(
        chat_handle: &Weak<types::ChatWindow>,
        app_state: Arc<Mutex<AppState>>,
        save_manager: Arc<Mutex<ChatSaveManager>>,
    ) {
        {
            let save_manager_clone = save_manager.clone();
            let chat_handle_clone = chat_handle.clone();
            
            if let Some(chat) = chat_handle.upgrade() {
                chat.on_refresh_saved_chats(move || {
                    if let Ok(chat_files) = save_manager_clone.lock().unwrap().get_saved_chats() {
                        let slint_items: Vec<ChatFileItem> = chat_files.iter()
                            .map(chat_file_to_slint_item)
                            .collect();
                        
                        if let Some(chat) = chat_handle_clone.upgrade() {
                            let model = Rc::new(VecModel::from(slint_items));
                            chat.set_saved_chats(ModelRc::from(model));
                        }
                    }
                });
            }
        }

        {
            let save_manager_clone = save_manager.clone();
            let app_state_clone = app_state.clone();
            let chat_handle_clone = chat_handle.clone();
            
            if let Some(chat) = chat_handle.upgrade() {
                chat.on_open_file_explorer_for_restore(move || {
                    if let Ok(Some(session)) = save_manager_clone.lock().unwrap().restore_chat_interactive() {
                        {
                            let state = app_state_clone.lock().unwrap();
                            let mut messages = state.messages.lock().unwrap();
                            messages.clear();
                            
                            for chat_msg in &session.messages {
                                let ui_message = types::ChatMessage {
                                    username: chat_msg.sender.clone().into(),
                                    content: chat_msg.content.clone().into(),
                                    timestamp: chat_msg.timestamp.format("%H:%M").to_string().into(),
                                    is_own: false,
                                    is_system: matches!(chat_msg.message_type, MessageType::System),
                                };
                                messages.push(ui_message);
                            }
                        }
                        
                        if let Some(chat) = chat_handle_clone.upgrade() {
                            chat.set_save_status("Chat restored successfully!".into());
                            crate::app::ui_handlers::update_messages(&chat_handle_clone, &app_state_clone);
                        }
                    }
                });
            }
        }

        {
            let save_manager_clone = save_manager.clone();
            let app_state_clone = app_state.clone();
            let chat_handle_clone = chat_handle.clone();
            
            if let Some(chat) = chat_handle.upgrade() {
                chat.on_restore_chat_from_path(move |file_path| {
                    let path = std::path::Path::new(file_path.as_str());
                    if let Ok(session) = save_manager_clone.lock().unwrap().load_chat_from_file(path) {
                        {
                            let state = app_state_clone.lock().unwrap();
                            let mut messages = state.messages.lock().unwrap();
                            messages.clear();
                            
                            for chat_msg in &session.messages {
                                let ui_message = types::ChatMessage {
                                    username: chat_msg.sender.clone().into(),
                                    content: chat_msg.content.clone().into(),
                                    timestamp: chat_msg.timestamp.format("%H:%M").to_string().into(),
                                    is_own: false,
                                    is_system: matches!(chat_msg.message_type, MessageType::System),
                                };
                                messages.push(ui_message);
                            }
                        }
                        
                        if let Some(chat) = chat_handle_clone.upgrade() {
                            chat.set_save_status(format!("Chat loaded from: {}", file_path).into());
                            crate::app::ui_handlers::update_messages(&chat_handle_clone, &app_state_clone);
                        }
                    }
                });
            }
        }

        {
            if let Some(chat) = chat_handle.upgrade() {
                chat.on_toggle_auto_save(move |enabled| {
                    println!("Auto-save toggled: {}", enabled);
                });
            }
        }
    }
}
