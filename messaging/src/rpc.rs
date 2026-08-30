use anyhow::{Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::user::{Member, Membership};

pub const AUTH_ALPN: &[u8] = b"p2p-chat/auth/0";
pub const MAX_FRAME: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Request {
    Register {
        localpart: String,
        password: String,
        device_name: String,
    },
    Login {
        localpart: String,
        password: String,
        device_name: String,
    },
    Logout {
        token: String,
    },
    Whoami {
        token: String,
    },
    SetDisplayName {
        token: String,
        display_name: String,
    },
    CreateRoom {
        token: String,
        name: String,
        invite_only: bool,
    },
    ListRooms {
        token: String,
    },
    RoomMembers {
        token: String,
        room: String,
    },
    JoinRoom {
        token: String,
        room: String,
    },
    LeaveRoom {
        token: String,
        room: String,
    },
    Invite {
        token: String,
        room: String,
        user_id: String,
    },
    Kick {
        token: String,
        room: String,
        user_id: String,
    },
    Ban {
        token: String,
        room: String,
        user_id: String,
    },
    Unban {
        token: String,
        room: String,
        user_id: String,
    },
    SetPowerLevel {
        token: String,
        room: String,
        user_id: String,
        level: i64,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoomSummary {
    pub id: String,
    pub name: String,
    pub ticket: String,
    pub invite_only: bool,
    pub membership: Membership,
    pub power_level: i64,
    pub member_count: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub user_id: String,
    pub device_id: String,
    pub display_name: String,
    pub token: String,
    pub server_name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Identity {
    pub user_id: String,
    pub device_id: String,
    pub display_name: String,
    pub server_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorCode {
    BadCredentials,
    UserExists,
    InvalidToken,
    Forbidden,
    NotFound,
    Invalid,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Response {
    Session(Session),
    Identity(Identity),
    Rooms(Vec<RoomSummary>),
    Room(RoomSummary),
    Members(Vec<Member>),
    Ok,
    Error { code: ErrorCode, message: String },
}

impl Response {
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Response::Error {
            code,
            message: message.into(),
        }
    }

    pub fn into_result(self) -> Result<Response> {
        match self {
            Response::Error { code, message } => bail!("{:?}: {}", code, message),
            other => Ok(other),
        }
    }
}

pub async fn write_frame<W, T>(writer: &mut W, value: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_FRAME {
        bail!("frame of {} bytes exceeds the limit", bytes.len());
    }
    writer.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_frame<R, T>(reader: &mut R) -> Result<T>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut length = [0u8; 4];
    reader.read_exact(&mut length).await?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME {
        bail!("frame of {} bytes exceeds the limit", length);
    }
    let mut buffer = vec![0u8; length];
    reader.read_exact(&mut buffer).await?;
    Ok(serde_json::from_slice(&buffer)?)
}
