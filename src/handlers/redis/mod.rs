use std::str::FromStr;
use limiter::RequestsLimiter;

pub mod limiter;

#[cfg(test)]
mod limiter_test;

pub struct RedisServices {
    pub connection_url: String,
    pub inline_request_limiter: RequestsLimiter,
}

impl RedisServices {
    pub fn from_env() -> Self {
        let host: String = resolve_mandatory_env("REDIS_HOST");
        let port: u16 = resolve_mandatory_env("REDIS_PORT");
        let password: String = resolve_mandatory_env("REDIS_PASSWORD");

        let connection_url =format!("redis://:{password}@{host}:{port}/");
        let redis_client = mobc_redis::redis::Client::open(connection_url.clone())
            .expect("Cannot connect to Redis");

        Self {
            connection_url,
            inline_request_limiter: RequestsLimiter::from_env(redis_client),
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
