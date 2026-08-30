use anyhow::Result;
use slint::{ComponentHandle, SharedString, Weak};
use std::sync::{Arc, Mutex, OnceLock};

use client::Net;

use crate::app::{
    app_state::AppState,
    networking::{cleanup_network_resources, send_goodbye, send_message},
    room_handlers::{open_peer_room, open_room, refresh_rooms},
    session::Session,
    types,
    ui_handlers::update_messages,
};

static SESSION: OnceLock<Mutex<Option<Arc<Session>>>> = OnceLock::new();
static NET: tokio::sync::OnceCell<Arc<Net>> = tokio::sync::OnceCell::const_new();

async fn net() -> Result<Arc<Net>> {
    NET.get_or_try_init(|| async { Net::start().await.map(Arc::new) })
        .await
        .cloned()
}

fn session_slot() -> &'static Mutex<Option<Arc<Session>>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

fn current_session() -> Option<Arc<Session>> {
    session_slot().lock().unwrap().clone()
}

fn set_session(session: Option<Arc<Session>>) {
    *session_slot().lock().unwrap() = session;
}

fn set_auth_status(handle: &Weak<types::AuthWindow>, message: String) {
    let handle = handle.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = handle.upgrade() {
            window.set_status(SharedString::from(message));
            window.set_busy(false);
        }
    });
}

fn set_rooms_status(handle: &Weak<types::RoomsWindow>, message: String) {
    let handle = handle.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = handle.upgrade() {
            window.set_status(SharedString::from(message));
        }
    });
}

pub struct App {}

impl App {
    pub fn run() -> Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;

        let auth = types::AuthWindow::new()?;
        let rooms = types::RoomsWindow::new()?;
        let chat = types::ChatWindow::new()?;

        let auth_handle = auth.as_weak();
        let rooms_handle = rooms.as_weak();
        let chat_handle = chat.as_weak();

        let app_state = Arc::new(Mutex::new(AppState::new()));

        let remembered = client::Credentials::load().unwrap_or_default();
        if let Some(server) = remembered.server_node_id.as_deref() {
            auth.set_ticket(SharedString::from(server));
        }
        if let Some(localpart) = remembered
            .user_id
            .as_deref()
            .and_then(messaging::user::localpart_of)
        {
            auth.set_username(SharedString::from(localpart));
        }
        if remembered.token.is_some() {
            auth.set_status(SharedString::from("Restoring your session..."));
        }

        Self::wire_auth(&auth_handle, &rooms_handle, rt.handle().clone());
        Self::wire_rooms(
            &auth_handle,
            &rooms_handle,
            &chat_handle,
            app_state.clone(),
            rt.handle().clone(),
        );
        Self::wire_peer_rooms(
            &auth_handle,
            &chat_handle,
            app_state.clone(),
            rt.handle().clone(),
        );
        Self::wire_chat(
            &auth_handle,
            &rooms_handle,
            &chat_handle,
            app_state.clone(),
            rt.handle().clone(),
        );

        let auth_for_restore = auth_handle.clone();
        let rooms_for_restore = rooms_handle.clone();
        let had_saved_session = remembered.token.is_some();
        rt.handle().spawn(async move {
            if !had_saved_session {
                return;
            }

            let net = match net().await {
                Ok(net) => net,
                Err(e) => {
                    eprintln!("could not start networking: {}", e);
                    return;
                }
            };

            match Session::restore(&net).await {
                Ok(Some(session)) => {
                    let session = Arc::new(session);
                    set_session(Some(session.clone()));

                    if let Err(e) = refresh_rooms(&session, rooms_for_restore.clone()).await {
                        eprintln!("failed to list rooms: {}", e);
                        return;
                    }

                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(rooms) = rooms_for_restore.upgrade() {
                            let _ = rooms.show();
                        }
                        if let Some(auth) = auth_for_restore.upgrade() {
                            let _ = auth.hide();
                        }
                    });
                }
                Ok(None) => {
                    set_auth_status(
                        &auth_for_restore,
                        "Your saved session has expired, sign in again".to_string(),
                    );
                }
                Err(e) => set_auth_status(
                    &auth_for_restore,
                    format!("Could not reach the server: {}", e),
                ),
            }
        });

        let _ = auth.show();
        let result = auth.run();

        rt.shutdown_timeout(std::time::Duration::from_secs(5));
        result.map_err(|e| anyhow::anyhow!("Slint error: {}", e))
    }

    fn wire_auth(
        auth_handle: &Weak<types::AuthWindow>,
        rooms_handle: &Weak<types::RoomsWindow>,
        rt: tokio::runtime::Handle,
    ) {
        let Some(auth) = auth_handle.upgrade() else {
            return;
        };

        let make_handler = |creating: bool| {
            let auth_handle = auth_handle.clone();
            let rooms_handle = rooms_handle.clone();
            let rt = rt.clone();

            move |ticket: SharedString, user: SharedString, password: SharedString| {
                let auth_handle = auth_handle.clone();
                let rooms_handle = rooms_handle.clone();
                let ticket = ticket.to_string();
                let user = user.to_string();
                let password = password.to_string();

                rt.spawn(async move {
                    let net = match net().await {
                        Ok(net) => net,
                        Err(e) => {
                            set_auth_status(&auth_handle, format!("{}", e));
                            return;
                        }
                    };

                    let session =
                        match Session::sign_in(&net, &ticket, &user, &password, creating).await {
                            Ok(session) => Arc::new(session),
                            Err(e) => {
                                set_auth_status(&auth_handle, format!("{}", e));
                                return;
                            }
                        };

                    set_session(Some(session.clone()));

                    if let Err(e) = refresh_rooms(&session, rooms_handle.clone()).await {
                        set_auth_status(&auth_handle, format!("{}", e));
                        return;
                    }

                    let auth_for_ui = auth_handle.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(rooms) = rooms_handle.upgrade() {
                            let _ = rooms.show();
                        }
                        if let Some(auth) = auth_for_ui.upgrade() {
                            auth.set_busy(false);
                            auth.set_password(SharedString::from(""));
                            auth.set_status(SharedString::from(""));
                            let _ = auth.hide();
                        }
                    });
                });
            }
        };

        auth.on_sign_in(make_handler(false));
        auth.on_create_account(make_handler(true));
    }

    fn wire_peer_rooms(
        auth_handle: &Weak<types::AuthWindow>,
        chat_handle: &Weak<types::ChatWindow>,
        app_state: Arc<Mutex<AppState>>,
        rt: tokio::runtime::Handle,
    ) {
        let Some(auth) = auth_handle.upgrade() else {
            return;
        };

        let auth_handle = auth_handle.clone();
        let chat_handle = chat_handle.clone();

        auth.on_start_peer_room(move |ticket, name| {
            let auth_handle = auth_handle.clone();
            let chat_handle = chat_handle.clone();
            let app_state = app_state.clone();
            let ticket = ticket.to_string();
            let name = name.to_string();

            rt.spawn(async move {
                let net = match net().await {
                    Ok(net) => net,
                    Err(e) => {
                        set_auth_status(&auth_handle, format!("{}", e));
                        return;
                    }
                };

                if let Err(e) = open_peer_room(
                    &net,
                    ticket,
                    name,
                    app_state,
                    chat_handle,
                    auth_handle.clone(),
                )
                .await
                {
                    set_auth_status(&auth_handle, format!("{}", e));
                }
            });
        });
    }

    fn wire_rooms(
        auth_handle: &Weak<types::AuthWindow>,
        rooms_handle: &Weak<types::RoomsWindow>,
        chat_handle: &Weak<types::ChatWindow>,
        app_state: Arc<Mutex<AppState>>,
        rt: tokio::runtime::Handle,
    ) {
        let Some(rooms) = rooms_handle.upgrade() else {
            return;
        };

        {
            let rooms_handle = rooms_handle.clone();
            let rt = rt.clone();
            rooms.on_refresh(move || {
                let Some(session) = current_session() else {
                    return;
                };
                let rooms_handle = rooms_handle.clone();
                rt.spawn(async move {
                    if let Err(e) = refresh_rooms(&session, rooms_handle.clone()).await {
                        set_rooms_status(&rooms_handle, format!("{}", e));
                    }
                });
            });
        }

        {
            let rooms_handle = rooms_handle.clone();
            let rt = rt.clone();
            rooms.on_create_room(move |name, invite_only| {
                let Some(session) = current_session() else {
                    return;
                };
                let rooms_handle = rooms_handle.clone();
                let name = name.to_string();

                rt.spawn(async move {
                    if let Err(e) = session.create_room(&name, invite_only).await {
                        set_rooms_status(&rooms_handle, format!("{}", e));
                        return;
                    }
                    if let Err(e) = refresh_rooms(&session, rooms_handle.clone()).await {
                        set_rooms_status(&rooms_handle, format!("{}", e));
                    }
                });
            });
        }

        {
            let rooms_handle = rooms_handle.clone();
            let chat_handle = chat_handle.clone();
            let app_state = app_state.clone();
            let rt = rt.clone();

            rooms.on_open_room(move |room| {
                let Some(session) = current_session() else {
                    return;
                };
                let rooms_handle = rooms_handle.clone();
                let chat_handle = chat_handle.clone();
                let app_state = app_state.clone();
                let room = room.to_string();

                rt.spawn(async move {
                    let net = match net().await {
                        Ok(net) => net,
                        Err(e) => {
                            set_rooms_status(&rooms_handle, format!("{}", e));
                            return;
                        }
                    };

                    if let Err(e) = open_room(
                        &net,
                        session,
                        room,
                        app_state,
                        chat_handle,
                        rooms_handle.clone(),
                    )
                    .await
                    {
                        set_rooms_status(&rooms_handle, format!("{}", e));
                    }
                });
            });
        }

        {
            let auth_handle = auth_handle.clone();
            let rooms_handle = rooms_handle.clone();
            let rt = rt.clone();

            rooms.on_sign_out(move || {
                let auth_handle = auth_handle.clone();
                let rooms_handle = rooms_handle.clone();

                rt.spawn(async move {
                    if let Some(session) = current_session() {
                        if let Err(e) = session.sign_out().await {
                            eprintln!("failed to sign out cleanly: {}", e);
                        }
                    }
                    set_session(None);

                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(auth) = auth_handle.upgrade() {
                            auth.set_busy(false);
                            auth.set_status(SharedString::from("Signed out"));
                            let _ = auth.show();
                        }
                        if let Some(rooms) = rooms_handle.upgrade() {
                            let _ = rooms.hide();
                        }
                    });
                });
            });
        }
    }

    fn wire_chat(
        auth_handle: &Weak<types::AuthWindow>,
        rooms_handle: &Weak<types::RoomsWindow>,
        chat_handle: &Weak<types::ChatWindow>,
        app_state: Arc<Mutex<AppState>>,
        rt: tokio::runtime::Handle,
    ) {
        let Some(chat) = chat_handle.upgrade() else {
            return;
        };

        {
            let app_state = app_state.clone();
            let chat_handle = chat_handle.clone();
            let rt = rt.clone();

            chat.on_send_message(move |message| {
                let app_state = app_state.clone();
                let chat_handle = chat_handle.clone();
                let message = message.to_string();

                rt.spawn(async move {
                    match send_message(message, app_state.clone()).await {
                        Ok(()) => update_messages(&chat_handle, &app_state),
                        Err(e) => eprintln!("failed to send: {}", e),
                    }
                });
            });
        }

        {
            let app_state = app_state.clone();
            let chat_handle = chat_handle.clone();
            let rooms_handle = rooms_handle.clone();
            let auth_handle = auth_handle.clone();
            let rt = rt.clone();

            chat.on_disconnect(move || {
                let app_state = app_state.clone();
                let chat_handle = chat_handle.clone();
                let rooms_handle = rooms_handle.clone();
                let auth_handle = auth_handle.clone();

                rt.spawn(async move {
                    let had_server = app_state.lock().unwrap().server_node_id.is_some();

                    if let Err(e) = send_goodbye(app_state.clone()).await {
                        eprintln!("failed to announce leaving: {}", e);
                    }

                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

                    if let Err(e) = cleanup_network_resources(app_state.clone()).await {
                        eprintln!("failed to clean up: {}", e);
                    }

                    if had_server {
                        if let Some(session) = current_session() {
                            let _ = refresh_rooms(&session, rooms_handle.clone()).await;
                        }
                    }

                    let _ = slint::invoke_from_event_loop(move || {
                        if had_server {
                            if let Some(rooms) = rooms_handle.upgrade() {
                                let _ = rooms.show();
                            }
                        } else if let Some(auth) = auth_handle.upgrade() {
                            auth.set_busy(false);
                            auth.set_status(SharedString::from(""));
                            let _ = auth.show();
                        }

                        if let Some(chat) = chat_handle.upgrade() {
                            let _ = chat.hide();
                        }
                    });
                });
            });
        }

        {
            let app_state = app_state.clone();
            chat.on_copy_session_token(move || {
                let token = app_state.lock().unwrap().current_session_token.clone();
                let Some(token) = token else {
                    eprintln!("no room ticket to copy");
                    return;
                };

                if let Err(e) = copy_to_clipboard(&token) {
                    eprintln!("could not copy the ticket: {}. Ticket: {}", e, token);
                }
            });
        }
    }
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let candidates: &[(&str, &[&str])] = &[
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
        ("pbcopy", &[]),
    ];

    for (program, args) in candidates {
        let Ok(mut child) = Command::new(program)
            .args(*args)
            .stdin(Stdio::piped())
            .spawn()
        else {
            continue;
        };

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
            drop(stdin);
        }

        if child.wait()?.success() {
            return Ok(());
        }
    }

    anyhow::bail!("no clipboard tool found, tried wl-copy, xclip, xsel and pbcopy")
}
