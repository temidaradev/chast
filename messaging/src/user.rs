use serde::{Deserialize, Serialize};

pub const POWER_ADMIN: i64 = 100;
pub const POWER_MODERATOR: i64 = 50;
pub const POWER_MEMBER: i64 = 0;

pub const POWER_TO_INVITE: i64 = POWER_MODERATOR;
pub const POWER_TO_KICK: i64 = POWER_MODERATOR;
pub const POWER_TO_BAN: i64 = POWER_MODERATOR;
pub const POWER_TO_SET_POWER: i64 = POWER_MODERATOR;

pub const MAX_LOCALPART: usize = 64;
pub const MIN_PASSWORD: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Membership {
    Invite,
    Join,
    Leave,
    Ban,
}

impl Membership {
    pub fn as_str(&self) -> &'static str {
        match self {
            Membership::Invite => "invite",
            Membership::Join => "join",
            Membership::Leave => "leave",
            Membership::Ban => "ban",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "invite" => Some(Membership::Invite),
            "join" => Some(Membership::Join),
            "leave" => Some(Membership::Leave),
            "ban" => Some(Membership::Ban),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub user_id: String,
    pub display_name: String,
    pub membership: Membership,
    pub power_level: i64,
}

pub fn power_label(level: i64) -> &'static str {
    if level >= POWER_ADMIN {
        "admin"
    } else if level >= POWER_MODERATOR {
        "moderator"
    } else {
        "member"
    }
}

pub fn build_user_id(localpart: &str, server_name: &str) -> String {
    format!("@{}:{}", localpart, server_name)
}

pub fn localpart_of(user_id: &str) -> Option<&str> {
    user_id.strip_prefix('@')?.split(':').next()
}

pub fn validate_localpart(localpart: &str) -> Result<(), String> {
    if localpart.is_empty() {
        return Err("username cannot be empty".to_string());
    }
    if localpart.len() > MAX_LOCALPART {
        return Err(format!(
            "username cannot be longer than {} characters",
            MAX_LOCALPART
        ));
    }
    if !localpart
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
    {
        return Err("username may only contain a-z, 0-9, dot, underscore and dash".to_string());
    }
    Ok(())
}

pub fn validate_password(password: &str) -> Result<(), String> {
    if password.chars().count() < MIN_PASSWORD {
        return Err(format!(
            "password must be at least {} characters",
            MIN_PASSWORD
        ));
    }
    Ok(())
}
