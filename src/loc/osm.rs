use async_trait::async_trait;
use reqwest::header::{ACCEPT_LANGUAGE, USER_AGENT};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use http_cache::{HitOrMiss, XCACHELOOKUP};
use http_cache_reqwest::{Cache, CacheMode, MokaManager, HttpCache, CacheOptions};
use prometheus::Opts;
use super::{LocFinder, LocResult, Location};
use crate::metrics;

pub struct OpenStreetMapLocFinder {
    client: ClientWithMiddleware,

    api_req_counter: prometheus::Counter,
    cached_resp_counter: prometheus::Counter,
    fetched_resp_counter: prometheus::Counter
}

impl OpenStreetMapLocFinder {
    pub fn new() -> OpenStreetMapLocFinder {
        let client = reqwest::Client::builder()
            .build().expect("couldn't create an HTTP client");
        let client = ClientBuilder::new(client)
            .with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager: MokaManager::default(),
                options: Some(CacheOptions::default()),
            }))
            .build();

        let api_req_opts = Opts::new("open_street_map_api_requests_total", "count of requests to the OpenStreetMap API");

        let resp_opts = Opts::new("open_street_map_api_responses_total", "count of responses from the OpenStreetMap API split by the source");
        let from_cache_opts = resp_opts.clone().const_label("source", "cache");
        let from_remote_opts = resp_opts.const_label("source", "remote");

        OpenStreetMapLocFinder {
            client,

            api_req_counter: metrics::REGISTRY.register_counter("OpenStreetMap API requests", api_req_opts),
            cached_resp_counter: metrics::REGISTRY.register_counter("OpenStreetMap API requests", from_cache_opts),
            fetched_resp_counter: metrics::REGISTRY.register_counter("OpenStreetMap API requests", from_remote_opts),
        }
    }
}

#[async_trait]
impl LocFinder for OpenStreetMapLocFinder {
    async fn find(&self, query: &str, lang_code: &str) -> LocResult {
        self.api_req_counter.inc();

        let url = format!("https://nominatim.openstreetmap.org/search?q={query}&format=json");
        let resp = self.client.get(url)
            .header(USER_AGENT, "kozalosev/LocPlaceBot")
            .header(ACCEPT_LANGUAGE, lang_code)
            .send()
            .await?;

        let resp_counter = resp.headers()
            .get(XCACHELOOKUP)
            .filter(|x| x.to_str().unwrap_or("") == HitOrMiss::HIT.to_string())
            .map(|_| &self.cached_resp_counter)
            .unwrap_or(&self.fetched_resp_counter);
        resp_counter.inc();

        let json = resp.json::<serde_json::Value>().await?;
        log::info!("response from Open Street Map Nominatim API: {json}");

        let results = json.as_array().unwrap().iter()
            .filter_map(map_resp)
            .collect();
        Ok(results)
    }
}

fn map_resp(v: &serde_json::Value) -> Option<Location> {
    let address = Some(v["display_name"].as_str()?.to_string());

    let latitude: f64 = v["lat"].as_str()?.parse().ok()?;
    let longitude: f64 = v["lon"].as_str()?.parse().ok()?;

    Some(Location {
        address, latitude, longitude
    })
}