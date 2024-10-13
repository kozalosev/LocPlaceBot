use std::error::Error;
use std::sync::Arc;
use async_trait::async_trait;
use derive_more::Constructor;
use http::Extensions;
use http_cache::{CacheManager, CacheMode, HitOrMiss, HttpCache, HttpCacheOptions, HttpResponse, XCACHE};
use http_cache_reqwest::Cache;
use http_cache_semantics::{CacheOptions, CachePolicy};
use mobc::Pool;
use mobc_redis::redis::AsyncCommands;
use mobc_redis::RedisConnectionManager;
use reqwest::header::HeaderValue;
use reqwest::{Body, Request, Response};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, Middleware, Next};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;

const X_BODY_HASH: &str = "X-Body-Hash";

pub fn caching_client(redis_pool: &Pool<RedisConnectionManager>) -> ClientWithMiddleware {
    caching_client_builder(redis_pool).build()
}

pub fn caching_client_builder(redis_pool: &Pool<RedisConnectionManager>) -> ClientBuilder {
    let client = reqwest::Client::builder()
        .build().expect("couldn't create an HTTP client");
    ClientBuilder::new(client)
        .with(InsertBodyHashIntoHeadersMiddleware)
        .with(Cache(HttpCache {
            mode: CacheMode::IgnoreRules,
            manager: RedisCacheManager::new(redis_pool.clone()),
            options: HttpCacheOptions {
                cache_options: Some(CacheOptions {
                    ignore_cargo_cult: true,
                    ..CacheOptions::default()
                }),
                cache_key: Some(Arc::new(|parts| {
                    let body_hash = parts.headers.get(X_BODY_HASH)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("no-body-hash");
                    format!("loc-cache:{}:{}:{}", parts.method, parts.uri, body_hash)
                })),
                ..HttpCacheOptions::default()
            },
        }))
}

struct InsertBodyHashIntoHeadersMiddleware;

#[async_trait]
impl Middleware for InsertBodyHashIntoHeadersMiddleware {
    async fn handle(&self, mut req: Request, extensions: &mut Extensions, next: Next<'_>) -> reqwest_middleware::Result<Response> {
        let maybe_body_hash = req.body()
            .and_then(Body::as_bytes)
            .map(sha256::digest)
            .and_then(|hash| HeaderValue::from_str(&hash).ok());
        if let Some(body_hash) = maybe_body_hash {
            req.headers_mut().insert(X_BODY_HASH, body_hash);
        }
        next.run(req, extensions).await
    }
}

#[derive(Clone, Constructor)]
struct RedisCacheManager {
    pool: Pool<RedisConnectionManager>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse,
    policy: CachePolicy,
}

#[async_trait]
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

    fn inc_resp_counter(&self, resp: &Response) {
        let resp_counter = if from_cache(resp) {
            self.cached_resp_counter()
        } else {
            self.fetched_resp_counter()
        };
        resp_counter.inc();
    }
}

fn from_cache(resp: &Response) -> bool {
    log::debug!("Response headers: {:?}", resp.headers());

    let hit = HitOrMiss::HIT.to_string();
    let predicate = |x: &&HeaderValue| {
        let value = x.to_str().unwrap_or("");
        value == hit
    };
    resp.headers()
        .get(XCACHE)
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
