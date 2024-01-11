use std::str::FromStr;
use mobc_redis::RedisConnectionManager;

pub fn redis_pool() -> mobc::Pool<RedisConnectionManager> {
    let host: String = resolve_mandatory_env("REDIS_HOST");
    let port: u16 = resolve_mandatory_env("REDIS_PORT");
    let password: String = resolve_mandatory_env("REDIS_PASSWORD");

    let client = mobc_redis::redis::Client::open(format!("redis://:{password}@{host}:{port}/"))
        .expect("Cannot connect to Redis");
    mobc::Pool::new(RedisConnectionManager::new(client))
}

pub fn resolve_mandatory_env<T: FromStr + ToString>(key: &str) -> T {
    let val = std::env::var(key)
        .expect(format!("{key} is not set but mandatory!").as_str());
    let val = T::from_str(val.as_str())
        .ok().expect(format!("Couldn't convert {key} for some reason").as_str());
    if key.to_lowercase().contains("password") {
        log::info!("{} is set to ***", key);
    } else {
        log::info!("{} is set to {}", key, val.to_string());
    }
    val
}

pub fn resolve_optional_env<T: FromStr + ToString>(key: &str, default: T) -> T {
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
