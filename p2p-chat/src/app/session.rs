use anyhow::{Context, Result, bail};
use client::{AuthClient, Credentials, Net};
use messaging::rpc::{Request, Response, RoomSummary};
use std::str::FromStr;
use ticket::Ticket;

pub struct Session {
    pub auth: AuthClient,
    pub credentials: Credentials,
}

impl Session {
    pub async fn sign_in(
        net: &Net,
        ticket: &str,
        localpart: &str,
        password: &str,
        creating: bool,
    ) -> Result<Self> {
        let mut credentials = Credentials::load()?;
        credentials.remember_server(ticket)?;

        let auth = AuthClient::from_endpoint(net.endpoint.clone(), credentials.server()?);

        let device_name = std::env::var("P2P_CHAT_DEVICE").unwrap_or_else(|_| "desktop".to_string());
        let request = if creating {
            Request::Register {
                localpart: localpart.to_string(),
                password: password.to_string(),
                device_name,
            }
        } else {
            Request::Login {
                localpart: localpart.to_string(),
                password: password.to_string(),
                device_name,
            }
        };

        let Response::Session(session) = auth.call_ok(request).await? else {
            bail!("the server sent an unexpected answer");
        };

        credentials.user_id = Some(session.user_id);
        credentials.device_id = Some(session.device_id);
        credentials.display_name = Some(session.display_name);
        credentials.token = Some(session.token);
        credentials.server_name = Some(session.server_name);
        credentials.save()?;

        Ok(Self { auth, credentials })
    }

    pub async fn restore(net: &Net) -> Result<Option<Self>> {
        let credentials = Credentials::load()?;
        if credentials.token.is_none() || credentials.server_node_id.is_none() {
            return Ok(None);
        }

        let auth = AuthClient::from_endpoint(net.endpoint.clone(), credentials.server()?);

        let token = credentials.token()?.to_string();
        match auth.call(Request::Whoami { token }).await {
            Ok(Response::Identity(_)) => Ok(Some(Self { auth, credentials })),
            _ => Ok(None),
        }
    }

    pub fn token(&self) -> Result<String> {
        Ok(self.credentials.token()?.to_string())
    }

    pub fn user_id(&self) -> String {
        self.credentials
            .user_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn display_name(&self) -> String {
        self.credentials
            .display_name
            .clone()
            .unwrap_or_else(|| self.user_id())
    }

    pub async fn rooms(&self) -> Result<Vec<RoomSummary>> {
        let Response::Rooms(rooms) = self
            .auth
            .call_ok(Request::ListRooms {
                token: self.token()?,
            })
            .await?
        else {
            bail!("the server sent an unexpected answer");
        };
        Ok(rooms)
    }

    pub async fn create_room(&self, name: &str, invite_only: bool) -> Result<RoomSummary> {
        let Response::Room(room) = self
            .auth
            .call_ok(Request::CreateRoom {
                token: self.token()?,
                name: name.to_string(),
                invite_only,
            })
            .await?
        else {
            bail!("the server sent an unexpected answer");
        };
        Ok(room)
    }

    pub async fn join_room(&self, room: &str) -> Result<RoomSummary> {
        let Response::Room(room) = self
            .auth
            .call_ok(Request::JoinRoom {
                token: self.token()?,
                room: room.to_string(),
            })
            .await?
        else {
            bail!("the server sent an unexpected answer");
        };
        Ok(room)
    }

    pub async fn members(&self, room: &str) -> Result<Vec<messaging::Member>> {
        let Response::Members(members) = self
            .auth
            .call_ok(Request::RoomMembers {
                token: self.token()?,
                room: room.to_string(),
            })
            .await?
        else {
            bail!("the server sent an unexpected answer");
        };
        Ok(members)
    }

    pub async fn open(&self, name: &str) -> Result<(RoomSummary, Ticket)> {
        let rooms = self.rooms().await?;
        let found = rooms
            .into_iter()
            .find(|room| room.name == name || room.id == name)
            .with_context(|| format!("no room called {}", name))?;

        let found = match found.membership {
            messaging::Membership::Join => found,
            _ => self.join_room(name).await?,
        };

        let ticket = Ticket::from_str(&found.ticket).context("the server sent a broken ticket")?;
        Ok((found, ticket))
    }

    pub async fn sign_out(&self) -> Result<()> {
        if let Ok(token) = self.token() {
            let _ = self.auth.call(Request::Logout { token }).await;
        }

        let mut credentials = self.credentials.clone();
        credentials.clear_session();
        credentials.save()?;
        Ok(())
    }
}
