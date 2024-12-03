use async_trait::async_trait;
use reqwest::header::{ACCEPT_LANGUAGE, USER_AGENT};
use reqwest_middleware::ClientWithMiddleware;
use prometheus::Opts;
use super::cache::WithCachedResponseCounters;
use super::{cache, LocFinder, LocResult, Location, get_bounds, SEARCH_RADIUS};
use crate::metrics;
use crate::redis::REDIS;

pub struct OpenStreetMapLocFinder {
    client: ClientWithMiddleware,

    api_req_counter: prometheus::Counter,
    cached_resp_counter: prometheus::Counter,
    fetched_resp_counter: prometheus::Counter
}

impl OpenStreetMapLocFinder {
    pub fn new() -> OpenStreetMapLocFinder {
        let api_req_opts = Opts::new("open_street_map_api_requests_total", "count of requests to the OpenStreetMap API");

        let resp_opts = Opts::new("open_street_map_api_responses_total", "count of responses from the OpenStreetMap API split by the source");
        let from_cache_opts = resp_opts.clone().const_label("source", "cache");
        let from_remote_opts = resp_opts.const_label("source", "remote");

        OpenStreetMapLocFinder {
            client: cache::caching_client(&REDIS.pool),

            api_req_counter: metrics::REGISTRY.register_counter("OpenStreetMap API requests", api_req_opts),
            cached_resp_counter: metrics::REGISTRY.register_counter("OpenStreetMap API requests", from_cache_opts),
            fetched_resp_counter: metrics::REGISTRY.register_counter("OpenStreetMap API requests", from_remote_opts),
        }
    }
}

#[async_trait]
impl LocFinder for OpenStreetMapLocFinder {
    async fn find(&self, query: &str, lang_code: &str, location: Option<(f64, f64)>) -> LocResult {
        self.api_req_counter.inc();
        let viewbox_part = location
            .map(|loc| get_bounds(loc, *SEARCH_RADIUS))
            .map(|(p1, p2)| format!("&viewbox={},{},{},{}", p1.1, p1.0, p2.1, p2.0))
            .unwrap_or_default();
        let url = format!("https://nominatim.openstreetmap.org/search?q={query}&format=json{viewbox_part}");
        log::debug!("Request: {url}");
        let resp = self.client.get(url)
            .header(USER_AGENT, "kozalosev/LocPlaceBot")
            .header(ACCEPT_LANGUAGE, lang_code)
            .send().await?;
        self.inc_resp_counter(&resp);

        let json = resp.json::<serde_json::Value>().await?;
        log::info!("Response from Open Street Map Nominatim API: {json}");

        let results = json.as_array().unwrap().iter()
            .filter_map(map_resp)
            .collect();
        Ok(results)
    }
}

impl WithCachedResponseCounters for OpenStreetMapLocFinder {
    fn cached_resp_counter(&self) -> &prometheus::Counter {
        &self.cached_resp_counter
    }

    fn fetched_resp_counter(&self) -> &prometheus::Counter {
        &self.fetched_resp_counter
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