use messaging::user::{
    Membership, POWER_TO_BAN, POWER_TO_INVITE, POWER_TO_KICK, POWER_TO_SET_POWER,
};

pub fn can_invite(actor_power: i64) -> bool {
    actor_power >= POWER_TO_INVITE
}

pub fn can_kick(actor_power: i64, target_power: i64) -> bool {
    actor_power >= POWER_TO_KICK && actor_power > target_power
}

pub fn can_ban(actor_power: i64, target_power: i64) -> bool {
    actor_power >= POWER_TO_BAN && actor_power > target_power
}

pub fn can_set_power(actor_power: i64, target_power: i64, new_level: i64) -> bool {
    actor_power >= POWER_TO_SET_POWER && actor_power > target_power && new_level <= actor_power
}

pub fn can_join(current: Option<Membership>, invite_only: bool) -> Result<(), &'static str> {
    match current {
        Some(Membership::Ban) => Err("you are banned from this room"),
        Some(Membership::Join) => Ok(()),
        Some(Membership::Invite) => Ok(()),
        _ if invite_only => Err("this room is invite only"),
        _ => Ok(()),
    }
}

pub fn can_speak(current: Option<Membership>) -> bool {
    matches!(current, Some(Membership::Join))
}

#[cfg(test)]
mod tests {
    use super::*;
    use messaging::user::{POWER_ADMIN, POWER_MEMBER, POWER_MODERATOR};

    #[test]
    fn members_cannot_moderate() {
        assert!(!can_invite(POWER_MEMBER));
        assert!(!can_kick(POWER_MEMBER, POWER_MEMBER));
        assert!(!can_ban(POWER_MEMBER, POWER_MEMBER));
    }

    #[test]
    fn moderators_act_on_members_but_not_peers_or_admins() {
        assert!(can_invite(POWER_MODERATOR));
        assert!(can_kick(POWER_MODERATOR, POWER_MEMBER));
        assert!(!can_kick(POWER_MODERATOR, POWER_MODERATOR));
        assert!(!can_kick(POWER_MODERATOR, POWER_ADMIN));
        assert!(!can_ban(POWER_MODERATOR, POWER_ADMIN));
    }

    #[test]
    fn admins_cannot_be_kicked_by_other_admins() {
        assert!(can_kick(POWER_ADMIN, POWER_MODERATOR));
        assert!(!can_kick(POWER_ADMIN, POWER_ADMIN));
    }

    #[test]
    fn power_cannot_be_granted_above_your_own() {
        assert!(can_set_power(POWER_ADMIN, POWER_MEMBER, POWER_MODERATOR));
        assert!(can_set_power(POWER_ADMIN, POWER_MEMBER, POWER_ADMIN));
        assert!(!can_set_power(POWER_MODERATOR, POWER_MEMBER, POWER_ADMIN));
        assert!(!can_set_power(
            POWER_MODERATOR,
            POWER_MODERATOR,
            POWER_MEMBER
        ));
    }

    #[test]
    fn bans_survive_an_invite_only_check() {
        assert!(can_join(Some(Membership::Ban), false).is_err());
        assert!(can_join(Some(Membership::Ban), true).is_err());
    }

    #[test]
    fn invite_only_rooms_need_an_invite() {
        assert!(can_join(None, true).is_err());
        assert!(can_join(Some(Membership::Leave), true).is_err());
        assert!(can_join(Some(Membership::Invite), true).is_ok());
        assert!(can_join(None, false).is_ok());
    }

    #[test]
    fn only_joined_members_may_speak() {
        assert!(can_speak(Some(Membership::Join)));
        assert!(!can_speak(Some(Membership::Invite)));
        assert!(!can_speak(Some(Membership::Leave)));
        assert!(!can_speak(Some(Membership::Ban)));
        assert!(!can_speak(None));
    }
}
