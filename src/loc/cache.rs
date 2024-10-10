use std::error::Error;
use derive_more::Constructor;
use http_cache::{CacheManager, CacheMode, HitOrMiss, HttpCache, HttpResponse, XCACHELOOKUP};
use http_cache_reqwest::{Cache, CacheOptions};
use http_cache_semantics::CachePolicy;
use mobc_redis::redis::AsyncCommands;
use reqwest::header::HeaderValue;
use reqwest::Url;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use serde::{Deserialize, Serialize};
use crate::redis::REDIS;

pub fn caching_client() -> ClientWithMiddleware {
    let client = reqwest::Client::builder()
        .build().expect("couldn't create an HTTP client");
    ClientBuilder::new(client)
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: RedisCacheManager::new(REDIS.pool.clone()),
            options: Some(CacheOptions::default()),
        }))
        .build()
}

const CACHE_KEY_PREFIX: &str = "loc-cache";

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
    async fn get(&self, method: &str, url: &Url) -> http_cache::Result<Option<(HttpResponse, CachePolicy)>> {
        let result = self.pool.get().await
            .inspect_err(log_failed_connection_error)?
            .get::<String, Option<Vec<u8>>>(redis_req_key(method, url)).await
            .inspect_err(|err| log::error!("Couldn't fetch a record from Redis: {err}"))
            .ok().flatten()
            .map(|data| bincode::deserialize::<Store>(&data))
            .and_then(|result| result
                .inspect_err(|err| log::error!("Couldn't deserialize the record fetched from Redis: {err}"))
                .ok())
            .map(|store| (store.response, store.policy));
        Ok(result)
    }

    async fn put(&self, method: &str, url: &Url, res: HttpResponse, policy: CachePolicy) -> http_cache::Result<HttpResponse> {
        let store = Store { response: res.clone(), policy };
        let data = bincode::serialize(&store)
            .inspect_err(|err| log::error!("Couldn't serialize the response: {err}"))?;
        self.pool
            .get().await
            .inspect_err(log_failed_connection_error)?
            .set(redis_req_key(method, url), data).await
            .inspect_err(|err| log::error!("Couldn't push a record into Redis: {err}"))?;
        Ok(res)
    }

    async fn delete(&self, method: &str, url: &Url) -> http_cache::Result<()> {
        self.pool.get().await
            .inspect_err(log_failed_connection_error)?
            .del::<String, ()>(redis_req_key(method, url)).await
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

fn redis_req_key(method: &str, url: &Url) -> String {
    format!("{CACHE_KEY_PREFIX}:{method}:{url}")
}

fn log_failed_connection_error(err: &impl Error) {
    log::error!("Couldn't get a Redis connection: {err}")
}
