use std::error::Error;
use std::sync::Arc;
use derive_more::Constructor;
use http_cache::{CacheManager, CacheMode, HitOrMiss, HttpCache, HttpCacheOptions, HttpResponse, XCACHELOOKUP};
use http_cache_reqwest::Cache;
use http_cache_semantics::{CacheOptions, CachePolicy};
use mobc_redis::redis::AsyncCommands;
use reqwest::header::HeaderValue;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;

pub fn caching_client(redis_pool: &mobc::Pool<mobc_redis::RedisConnectionManager>) -> ClientWithMiddleware {
    let client = reqwest::Client::builder()
        .build().expect("couldn't create an HTTP client");
    let cb = ClientBuilder::new(client)
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: RedisCacheManager::new(redis_pool.clone()),
            options: HttpCacheOptions {
                cache_options: Some(CacheOptions {
                    ignore_cargo_cult: true,
                    ..CacheOptions::default()
                }),
                cache_key: Some(Arc::new(|parts| format!("loc-cache:{}:{}", parts.method, parts.uri))),
                ..HttpCacheOptions::default()
            },
        }));
    #[cfg(test)] let cb = cb.with(test_utils::RequestStoppingCounter);
    cb.build()
}

#[derive(Clone, Constructor)]
struct RedisCacheManager {
    pool: mobc::Pool<mobc_redis::RedisConnectionManager>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse,
    policy: CachePolicy,
}

#[async_trait::async_trait]
impl CacheManager for RedisCacheManager {
    async fn get(&self, cache_key: &str) -> http_cache::Result<Option<(HttpResponse, CachePolicy)>> {
        let result = self.pool.get().await
            .inspect_err(log_failed_connection_error)?
            .get::<&str, Option<Vec<u8>>>(cache_key).await
            .inspect_err(|err| log::error!("Couldn't fetch a record from Redis: {err}"))
            .ok().flatten()
            .map(deserialize)
            .and_then(|result| result
                .inspect_err(|err| log::error!("Couldn't deserialize the record fetched from Redis: {err}"))
                .ok())
            .map(|store: Store| (store.response, store.policy));
        Ok(result)
    }

    async fn put(&self, cache_key: String, res: HttpResponse, policy: CachePolicy) -> http_cache::Result<HttpResponse> {
        let store = Store { response: res.clone(), policy };
        let data = serialize(&store)
            .inspect_err(|err| log::error!("Couldn't serialize the response: {err}"))?;
        self.pool
            .get().await
            .inspect_err(log_failed_connection_error)?
            .set(cache_key, data).await
            .inspect_err(|err| log::error!("Couldn't push a record into Redis: {err}"))?;
        Ok(res)
    }

    async fn delete(&self, cache_key: &str) -> http_cache::Result<()> {
        self.pool.get().await
            .inspect_err(log_failed_connection_error)?
            .del::<&str, ()>(cache_key).await
            .inspect_err(|err| log::error!("Couldn't delete the record from Redis: {err}"))
            .map_err(Into::into)
    }
}

pub trait WithCachedResponseCounters {
    fn cached_resp_counter(&self) -> &prometheus::Counter;
    fn fetched_resp_counter(&self) -> &prometheus::Counter;

    fn inc_resp_counter(&self, resp: &reqwest::Response) {
        let resp_counter = if from_cache(resp) {
            self.cached_resp_counter()
        } else {
            self.fetched_resp_counter()
        };
        resp_counter.inc();
    }
}

fn from_cache(resp: &reqwest::Response) -> bool {
    log::debug!("Response headers: {:?}", resp.headers());

    let hit = HitOrMiss::HIT.to_string();
    let predicate = |x: &&HeaderValue| {
        let value = x.to_str().unwrap_or("");
        value == hit
    };
    resp.headers()
        .get(XCACHELOOKUP)
        .filter(predicate)
        .is_some()
}

fn log_failed_connection_error(err: &impl Error) {
    log::error!("Couldn't get a Redis connection: {err}")
}

fn serialize(value: impl Serialize) -> Result<Vec<u8>, bincode::error::EncodeError> {
    bincode::serde::encode_to_vec(value, bincode::config::standard())
}

fn deserialize<T: DeserializeOwned>(bytes: Vec<u8>) -> Result<T, bincode::error::DecodeError> {
    bincode::serde::decode_from_slice(bytes.as_slice(), bincode::config::standard())
        .map(|(t, _)| t)
}

#[cfg(test)]
pub mod test_utils {
    use axum::http::Extensions;
    use http::header::CACHE_CONTROL;
    use http::HeaderValue;
    use reqwest::{Body, Request};
    use reqwest_middleware::{Middleware, Next};
    
    pub static mut COUNTER: usize = 0;

    pub(super) struct RequestStoppingCounter;
    
    #[async_trait::async_trait]
    impl Middleware for RequestStoppingCounter {
        async fn handle(&self, _req: Request, _extensions: &mut Extensions, _next: Next<'_>) -> reqwest_middleware::Result<reqwest::Response> {
            unsafe { COUNTER += 1; }

            let mut resp = http::Response::new(Body::from("200 OK"));
            let headers = resp.headers_mut();
            headers.insert(CACHE_CONTROL, HeaderValue::from_static("max-age=604800"));
            Ok(reqwest::Response::from(resp))
        }
    }
}
