use redis_derive::ToRedisArgs;
use teloxide::types::UserId;

#[derive(ToRedisArgs, Copy, Clone)]
pub(super) struct UID {
    user_id: u64
}

impl From<UserId> for UID {
    fn from(value: UserId) -> Self {
        Self { user_id: value.0 }
    }
}

impl Into<UserId> for UID {
    fn into(self) -> UserId {
        UserId(self.user_id)
    }
}
