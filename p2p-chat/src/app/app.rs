use anyhow::Result;
use slint::{ComponentHandle, Weak};
use std::{sync::{Arc, Mutex}};

use crate::app::{
    app_state::AppState,
    networking::{send_message, send_disconnect},
    room_handlers::{create_room, join_room},
    types,
    ui_handlers::update_messages,
};

pub struct App {}

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

        Self::setup_navigation(&main_handle, &join_handle, &create_handle, &chat_handle);

        Self::setup_networking_callbacks(
            &main_handle,
            &join_handle,
            &create_handle,
            &chat_handle,
            app_state.clone(),
            rt.handle().clone(),
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

            if let Some(chat) = chat_handle.upgrade() {
                chat.on_send_message(move |message| {
                    println!("DEBUG: Sending message: '{}'", message);
                    let app_state = app_state_clone.clone();
                    let chat_handle = chat_handle_clone.clone();
                    let message = message.to_string();

                    rt_handle_clone.spawn(async move {
                        match send_message(message.clone(), app_state.clone()).await {
                            Ok(_) => {
                                println!("DEBUG: Message sent successfully, updating UI");
                                update_messages(&chat_handle, &app_state);
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
}
