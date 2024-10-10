use std::str::FromStr;
use mobc::Pool;
use mobc_redis::redis::Client;
use mobc_redis::RedisConnectionManager;
use once_cell::sync::Lazy;

pub static REDIS: Lazy<RedisConnection> = Lazy::new(RedisConnection::from_env);

pub struct RedisConnection {
    pub connection_url: String,
    pub pool: Pool<RedisConnectionManager>,
}

impl RedisConnection {
    pub fn from_env() -> Self {
        let host: String = resolve_mandatory_env("REDIS_HOST");
        let port: u16 = resolve_mandatory_env("REDIS_PORT");
        let password: String = resolve_mandatory_env("REDIS_PASSWORD");

        let connection_url =format!("redis://:{password}@{host}:{port}/");
        let client = Client::open(connection_url.clone())
            .expect("Cannot connect to Redis");
        let manager = RedisConnectionManager::new(client);
        let pool = Pool::new(manager);

        Self {
            connection_url,
            pool,
        }
    }
}

fn resolve_mandatory_env<T: FromStr + ToString>(key: &str) -> T {
    let val = std::env::var(key)
        .unwrap_or_else(|_| panic!("{key} is not set but mandatory!"));
    let val = T::from_str(val.as_str())
        .ok().unwrap_or_else(|| panic!("Couldn't convert {key} for some reason"));
    if key.to_lowercase().contains("password") {
        log::info!("{} is set to ***", key);
    } else {
        log::info!("{} is set to {}", key, val.to_string());
    }
    val
}
