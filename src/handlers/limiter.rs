use std::str::FromStr;
use anyhow::anyhow;
use mobc_redis::{redis, RedisConnectionManager};
use teloxide::types::{CallbackQuery, InlineQuery, Message, UserId};

const REDIS_KEY_PREFIX: &str = "rate-limiter.";

pub struct RequestsLimiter {
    pool: mobc::Pool<RedisConnectionManager>,
    max_allowed: i32,
    timeframe: usize,
}

impl RequestsLimiter {
    pub fn new(client: redis::Client, max_allowed: i32, timeframe: usize) -> Self {
        let manager = RedisConnectionManager::new(client);
        let pool = mobc::Pool::new(manager);
        RequestsLimiter { pool, max_allowed, timeframe }
    }

    pub fn from_env() -> Self {
        let host: String = resolve_mandatory_env("REDIS_HOST");
        let port: u16 = resolve_mandatory_env("REDIS_PORT");
        let password: String = resolve_mandatory_env("REDIS_PASSWORD");
        let max_allowed = resolve_optional_env("REQUESTS_LIMITER_MAX_ALLOWED", 10);
        let timeframe = resolve_optional_env("REQUESTS_LIMITER_TIMEFRAME", 60);

        let client = redis::Client::open(format!("redis://:{password}@{host}:{port}/"))
            .expect("Cannot connect to Redis");
        Self::new(client, max_allowed, timeframe)
    }

    pub async fn is_req_allowed(&self,  entity: &impl GetUserId) -> bool {
        if let Some(uid) = entity.user_id() {
            self.check(uid).await
                .unwrap_or_else(|e| {
                    log::error!("couldn't check limits for {uid}: {e}");
                    true
                })
        } else {
            log::warn!("no user_id");
            true
        }
    }

    async fn check(&self, uid: UserId) -> anyhow::Result<bool> {
        let key = REDIS_KEY_PREFIX.to_string() + uid.to_string().as_str();
        let req_count = self.fetch_requests_count(key).await?;

        log::debug!("Ordinal number of request is {req_count}");
        Ok(req_count <= self.max_allowed)
    }

    async fn fetch_requests_count(&self, key: String) -> anyhow::Result<i32> {
        let mut conn = self.pool
            .get().await?
            .into_inner();

        let redis::Value::Bulk(new_val) = redis::pipe().atomic()
            .incr(key.clone(), 1)
            .expire(key, self.timeframe).ignore()
            .query_async(&mut conn).await?
            else {
                return Err(anyhow!("unexpected non-bulk type of new_val"))
            };
        let redis::Value::Int(new_val) = new_val.get(0)
            .ok_or(anyhow!("unexpected empty vector in a bulk"))?
            else {
                return Err(anyhow!("unexpected non-int type of new_val"))
            };
        i32::try_from(*new_val)
            .map_err(|e| e.into())
    }
}

pub trait GetUserId {
    #[must_use]
    fn user_id(&self) -> Option<UserId>;
}

impl GetUserId for Message {
    fn user_id(&self) -> Option<UserId> {
        self.from()
            .map(|u| u.id)
    }
}

impl GetUserId for CallbackQuery {
    fn user_id(&self) -> Option<UserId> {
        Some(self.from.id)
    }
}

impl GetUserId for InlineQuery {
    fn user_id(&self) -> Option<UserId> {
        Some(self.from.id)
    }
}

fn resolve_mandatory_env<T: FromStr + ToString>(key: &str) -> T {
    let val = std::env::var(key)
        .expect(format!("{key} is not set but mandatory!").as_str());
    let val = T::from_str(val.as_str())
        .ok().expect(format!("Couldn't convert {key} for some reason").as_str());
    log::info!("{} is set to {}", key, val.to_string());
    val
}

fn resolve_optional_env<T: FromStr + ToString>(key: &str, default: T) -> T {
    let val = std::env::var(key)
        .unwrap_or(default.to_string())
        .parse::<T>()
        .unwrap_or_else(|_| {
            log::error!("Invalid value of {key}");
            default
        });
    log::info!("{} is set to {}", key, val.to_string());
    val
}
