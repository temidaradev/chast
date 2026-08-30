use iroh::{
    NodeId,
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use messaging::{
    rpc::{ErrorCode, Identity, Request, Response, RoomSummary, Session, read_frame, write_frame},
    user::{Member, Membership, POWER_ADMIN, POWER_MEMBER},
};
use ticket::Ticket;

use crate::{
    perms::{can_ban, can_invite, can_join, can_kick, can_set_power},
    store::{Device, Store},
};

#[derive(Debug, Clone)]
pub struct AuthProtocol {
    store: Store,
    server_name: String,
    node_id: NodeId,
    new_rooms: tokio::sync::mpsc::UnboundedSender<crate::store::Room>,
}

impl AuthProtocol {
    pub fn new(
        store: Store,
        server_name: String,
        node_id: NodeId,
        new_rooms: tokio::sync::mpsc::UnboundedSender<crate::store::Room>,
    ) -> Self {
        Self {
            store,
            server_name,
            node_id,
            new_rooms,
        }
    }

    fn authenticate(&self, token: &str, from: NodeId) -> Result<Device, Response> {
        match self.store.device_for_token(token) {
            Ok(Some(device)) => {
                if let Err(e) = self.store.bind_device_node(&device.device_id, from) {
                    eprintln!("failed to bind device to node: {}", e);
                }
                Ok(device)
            }
            Ok(None) => Err(Response::error(
                ErrorCode::InvalidToken,
                "this session is not valid, log in again",
            )),
            Err(e) => Err(Response::error(ErrorCode::Invalid, e.to_string())),
        }
    }

    fn summary(
        &self,
        room: &crate::store::Room,
        membership: Membership,
        power_level: i64,
    ) -> anyhow::Result<RoomSummary> {
        let ticket = Ticket {
            topic: room.topic,
            nodes: vec![crate::node_addr(self.node_id)],
        };

        Ok(RoomSummary {
            id: room.id.clone(),
            name: room.name.clone(),
            ticket: ticket.to_string(),
            invite_only: room.invite_only,
            membership,
            power_level,
            member_count: self.store.joined_count(&room.id)?,
        })
    }

    async fn dispatch(&self, request: Request, from: NodeId) -> Response {
        match self.handle(request, from).await {
            Ok(response) => response,
            Err(e) => Response::error(ErrorCode::Invalid, e.to_string()),
        }
    }

    async fn handle(&self, request: Request, from: NodeId) -> anyhow::Result<Response> {
        match request {
            Request::Register {
                localpart,
                password,
                device_name,
            } => {
                if self
                    .store
                    .find_user(&messaging::user::build_user_id(&localpart, &self.server_name))?
                    .is_some()
                {
                    return Ok(Response::error(
                        ErrorCode::UserExists,
                        format!("{} is already taken", localpart),
                    ));
                }

                let user = match self
                    .store
                    .register_user(&localpart, &password, &self.server_name)
                {
                    Ok(user) => user,
                    Err(e) => return Ok(Response::error(ErrorCode::Invalid, e.to_string())),
                };

                let (device, token) = self.store.create_device(&user.user_id, &device_name)?;
                self.store.bind_device_node(&device.device_id, from)?;

                println!("registered {} on device {}", user.user_id, device.device_id);

                Ok(Response::Session(Session {
                    user_id: user.user_id,
                    device_id: device.device_id,
                    display_name: user.display_name,
                    token,
                    server_name: self.server_name.clone(),
                }))
            }

            Request::Login {
                localpart,
                password,
                device_name,
            } => {
                let Some(user) = self.store.check_password(&localpart, &password)? else {
                    return Ok(Response::error(
                        ErrorCode::BadCredentials,
                        "wrong username or password",
                    ));
                };

                self.store.forget_devices_for_node(&user.user_id, from)?;
                let (device, token) = self.store.create_device(&user.user_id, &device_name)?;
                self.store.bind_device_node(&device.device_id, from)?;

                println!("{} logged in on device {}", user.user_id, device.device_id);

                Ok(Response::Session(Session {
                    user_id: user.user_id,
                    device_id: device.device_id,
                    display_name: user.display_name,
                    token,
                    server_name: self.server_name.clone(),
                }))
            }

            Request::Logout { token } => {
                self.store.revoke_token(&token)?;
                Ok(Response::Ok)
            }

            Request::Whoami { token } => {
                let device = match self.authenticate(&token, from) {
                    Ok(device) => device,
                    Err(response) => return Ok(response),
                };

                Ok(Response::Identity(Identity {
                    display_name: self.store.display_name(&device.user_id)?,
                    user_id: device.user_id,
                    device_id: device.device_id,
                    server_name: self.server_name.clone(),
                }))
            }

            Request::SetDisplayName {
                token,
                display_name,
            } => {
                let device = match self.authenticate(&token, from) {
                    Ok(device) => device,
                    Err(response) => return Ok(response),
                };

                if let Err(e) = self.store.set_display_name(&device.user_id, &display_name) {
                    return Ok(Response::error(ErrorCode::Invalid, e.to_string()));
                }
                Ok(Response::Ok)
            }

            Request::CreateRoom {
                token,
                name,
                invite_only,
            } => {
                let device = match self.authenticate(&token, from) {
                    Ok(device) => device,
                    Err(response) => return Ok(response),
                };

                let room = match self.store.create_room(&name, invite_only, &device.user_id) {
                    Ok(room) => room,
                    Err(e) => return Ok(Response::error(ErrorCode::Invalid, e.to_string())),
                };

                println!("{} created room {} ({})", device.user_id, room.name, room.id);

                if self.new_rooms.send(room.clone()).is_err() {
                    return Ok(Response::error(
                        ErrorCode::Invalid,
                        "the room was created but the server is not hosting it, restart the server",
                    ));
                }

                Ok(Response::Room(self.summary(
                    &room,
                    Membership::Join,
                    POWER_ADMIN,
                )?))
            }

            Request::ListRooms { token } => {
                let device = match self.authenticate(&token, from) {
                    Ok(device) => device,
                    Err(response) => return Ok(response),
                };

                let mut summaries = Vec::new();
                for (room, membership, power) in self.store.rooms_for_user(&device.user_id)? {
                    summaries.push(self.summary(&room, membership, power)?);
                }
                Ok(Response::Rooms(summaries))
            }

            Request::RoomMembers { token, room } => {
                let (device, room) = match self.resolve(&token, &room, from)? {
                    Ok(pair) => pair,
                    Err(response) => return Ok(response),
                };

                if self.store.membership(&room.id, &device.user_id)?.is_none() {
                    return Ok(Response::error(
                        ErrorCode::Forbidden,
                        "you are not a member of this room",
                    ));
                }

                Ok(Response::Members(self.store.members(&room.id)?))
            }

            Request::JoinRoom { token, room } => {
                let (device, room) = match self.resolve(&token, &room, from)? {
                    Ok(pair) => pair,
                    Err(response) => return Ok(response),
                };

                let current = self.store.membership(&room.id, &device.user_id)?;
                if let Err(reason) = can_join(current.map(|(state, _)| state), room.invite_only) {
                    return Ok(Response::error(ErrorCode::Forbidden, reason));
                }

                let power = current.map(|(_, power)| power).unwrap_or(POWER_MEMBER);
                self.store.set_membership(
                    &room.id,
                    &device.user_id,
                    Membership::Join,
                    power,
                    None,
                )?;

                println!("{} joined {}", device.user_id, room.name);
                Ok(Response::Room(self.summary(
                    &room,
                    Membership::Join,
                    power,
                )?))
            }

            Request::LeaveRoom { token, room } => {
                let (device, room) = match self.resolve(&token, &room, from)? {
                    Ok(pair) => pair,
                    Err(response) => return Ok(response),
                };

                let power = self.store.power_level(&room.id, &device.user_id)?;
                self.store.set_membership(
                    &room.id,
                    &device.user_id,
                    Membership::Leave,
                    power,
                    None,
                )?;

                println!("{} left {}", device.user_id, room.name);
                Ok(Response::Ok)
            }

            Request::Invite {
                token,
                room,
                user_id,
            } => {
                let (device, room) = match self.resolve(&token, &room, from)? {
                    Ok(pair) => pair,
                    Err(response) => return Ok(response),
                };

                let actor_power = self.store.power_level(&room.id, &device.user_id)?;
                if !self.store.is_joined(&room.id, &device.user_id)? {
                    return Ok(Response::error(
                        ErrorCode::Forbidden,
                        "you are not a member of this room",
                    ));
                }
                if !can_invite(actor_power) {
                    return Ok(Response::error(
                        ErrorCode::Forbidden,
                        "you need to be a moderator to invite people",
                    ));
                }
                if self.store.find_user(&user_id)?.is_none() {
                    return Ok(Response::error(
                        ErrorCode::NotFound,
                        format!("no user called {}", user_id),
                    ));
                }

                match self.store.membership(&room.id, &user_id)? {
                    Some((Membership::Ban, _)) => {
                        return Ok(Response::error(
                            ErrorCode::Forbidden,
                            "that user is banned, unban them first",
                        ));
                    }
                    Some((Membership::Join, _)) => return Ok(Response::Ok),
                    _ => {}
                }

                self.store.set_membership(
                    &room.id,
                    &user_id,
                    Membership::Invite,
                    POWER_MEMBER,
                    Some(&device.user_id),
                )?;

                println!("{} invited {} to {}", device.user_id, user_id, room.name);
                Ok(Response::Ok)
            }

            Request::Kick {
                token,
                room,
                user_id,
            } => {
                let (device, room) = match self.resolve(&token, &room, from)? {
                    Ok(pair) => pair,
                    Err(response) => return Ok(response),
                };

                let actor_power = self.store.power_level(&room.id, &device.user_id)?;
                let target_power = self.store.power_level(&room.id, &user_id)?;

                if !can_kick(actor_power, target_power) {
                    return Ok(Response::error(
                        ErrorCode::Forbidden,
                        "you cannot kick that user",
                    ));
                }

                self.store.set_membership(
                    &room.id,
                    &user_id,
                    Membership::Leave,
                    target_power,
                    None,
                )?;

                println!("{} kicked {} from {}", device.user_id, user_id, room.name);
                Ok(Response::Ok)
            }

            Request::Ban {
                token,
                room,
                user_id,
            } => {
                let (device, room) = match self.resolve(&token, &room, from)? {
                    Ok(pair) => pair,
                    Err(response) => return Ok(response),
                };

                let actor_power = self.store.power_level(&room.id, &device.user_id)?;
                let target_power = self.store.power_level(&room.id, &user_id)?;

                if !can_ban(actor_power, target_power) {
                    return Ok(Response::error(
                        ErrorCode::Forbidden,
                        "you cannot ban that user",
                    ));
                }

                self.store.set_membership(
                    &room.id,
                    &user_id,
                    Membership::Ban,
                    target_power,
                    None,
                )?;

                println!("{} banned {} from {}", device.user_id, user_id, room.name);
                Ok(Response::Ok)
            }

            Request::Unban {
                token,
                room,
                user_id,
            } => {
                let (device, room) = match self.resolve(&token, &room, from)? {
                    Ok(pair) => pair,
                    Err(response) => return Ok(response),
                };

                let actor_power = self.store.power_level(&room.id, &device.user_id)?;
                let target_power = self.store.power_level(&room.id, &user_id)?;
                if !can_ban(actor_power, target_power) {
                    return Ok(Response::error(
                        ErrorCode::Forbidden,
                        "you cannot unban that user",
                    ));
                }

                if !matches!(
                    self.store.membership(&room.id, &user_id)?,
                    Some((Membership::Ban, _))
                ) {
                    return Ok(Response::error(
                        ErrorCode::NotFound,
                        "that user is not banned",
                    ));
                }

                self.store.set_membership(
                    &room.id,
                    &user_id,
                    Membership::Leave,
                    target_power,
                    None,
                )?;

                println!("{} unbanned {} in {}", device.user_id, user_id, room.name);
                Ok(Response::Ok)
            }

            Request::SetPowerLevel {
                token,
                room,
                user_id,
                level,
            } => {
                let (device, room) = match self.resolve(&token, &room, from)? {
                    Ok(pair) => pair,
                    Err(response) => return Ok(response),
                };

                let actor_power = self.store.power_level(&room.id, &device.user_id)?;
                let Some((state, target_power)) = self.store.membership(&room.id, &user_id)? else {
                    return Ok(Response::error(
                        ErrorCode::NotFound,
                        "that user is not a member of this room",
                    ));
                };

                if !can_set_power(actor_power, target_power, level) {
                    return Ok(Response::error(
                        ErrorCode::Forbidden,
                        "you cannot set that power level",
                    ));
                }

                self.store
                    .set_membership(&room.id, &user_id, state, level, None)?;

                println!(
                    "{} set {} to power {} in {}",
                    device.user_id, user_id, level, room.name
                );
                Ok(Response::Ok)
            }
        }
    }

    fn resolve(
        &self,
        token: &str,
        room: &str,
        from: NodeId,
    ) -> anyhow::Result<Result<(Device, crate::store::Room), Response>> {
        let device = match self.authenticate(token, from) {
            Ok(device) => device,
            Err(response) => return Ok(Err(response)),
        };

        let Some(room) = self.store.find_room(room)? else {
            return Ok(Err(Response::error(
                ErrorCode::NotFound,
                "no room by that name",
            )));
        };

        Ok(Ok((device, room)))
    }
}

impl ProtocolHandler for AuthProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let from = connection.remote_node_id()?;

        loop {
            let (mut send, mut recv) = match connection.accept_bi().await {
                Ok(streams) => streams,
                Err(_) => break,
            };

            let request: Request = match read_frame(&mut recv).await {
                Ok(request) => request,
                Err(e) => {
                    eprintln!("malformed request from {}: {}", from.fmt_short(), e);
                    continue;
                }
            };

            let response = self.dispatch(request, from).await;

            if let Err(e) = write_frame(&mut send, &response).await {
                eprintln!("failed to answer {}: {}", from.fmt_short(), e);
                continue;
            }
            let _ = send.finish();
        }

        connection.closed().await;
        Ok(())
    }
}

pub fn describe_member(member: &Member) -> String {
    format!(
        "{:<28} {:<10} {:<10} {}",
        member.user_id,
        member.membership.as_str(),
        messaging::user::power_label(member.power_level),
        member.display_name
    )
}
