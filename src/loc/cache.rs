use http_cache::{CacheMode, HitOrMiss, HttpCache, MokaManager, XCACHELOOKUP};
use http_cache_reqwest::{Cache, CacheOptions};
use reqwest::header::HeaderValue;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};

pub fn caching_client() -> ClientWithMiddleware {
    let client = reqwest::Client::builder()
        .build().expect("couldn't create an HTTP client");
    let client = ClientBuilder::new(client)
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: MokaManager::default(),
            options: Some(CacheOptions::default()),
        }))
        .build();
    client
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
