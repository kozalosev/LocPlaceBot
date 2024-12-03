use std::str::FromStr;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use reqwest_middleware::ClientWithMiddleware;
use strum_macros::EnumString;
use serde::Serialize;
use serde_json::json;
use super::cache::WithCachedResponseCounters;
use super::{cache, Location, LocFinder, LocResult, SEARCH_RADIUS, SearchParams};
use crate::metrics;
use crate::redis::REDIS;

const FINDER_ENV_API_KEY: &str = "GOOGLE_MAPS_API_KEY";

static GAPI_MODE: Lazy<GoogleAPIMode> = Lazy::new(|| {
    let val = std::env::var("GAPI_MODE").expect("GAPI_MODE must be set!");
    log::info!("GAPI_MODE is {val}");
    GoogleAPIMode::from_str(val.as_str()).expect("Invalid value of GAPI_MODE")
});

#[derive(EnumString)]
pub enum GoogleAPIMode {
    Text,       // Text Search request
    GeoText,    // Geocoding request first, Text Search if ZERO_RESULTS
}

/// Load and check required parameters at startup
pub fn preload_env_vars() {
    let _ = *GAPI_MODE;
}

pub struct GoogleLocFinder {
    client: ClientWithMiddleware,
    api_key: String,

    geocode_req_counter: prometheus::Counter,
    text_req_counter: prometheus::Counter,
    cached_resp_counter: prometheus::Counter,
    fetched_resp_counter: prometheus::Counter,
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct SearchQuery {
    text_query: String,
    language_code: String,
    location_bias: Option<serde_json::Value>
}

impl SearchQuery {
    fn new(address: &str, lang_code: &str, location: Option<(f64, f64)>) -> Self {
        let viewport = location
            .map(|(lat, lng)| json!({
                "circle": {
                    "center": {
                        "latitude": lat,
                        "longitude": lng
                    },
                    "radius": *SEARCH_RADIUS
                }
            }));
        Self {
            text_query: address.to_string(),
            language_code: lang_code.to_string(),
            location_bias: viewport
        }
    }
}

impl GoogleLocFinder {
    pub fn init(api_key: &str) -> GoogleLocFinder {
        let base_opts = prometheus::Opts::new("google_maps_api_requests_total", "count of requests to the Google Maps API");
        let geocode_opts = base_opts.clone().const_label("API", "geocode");
        let text_opts    = base_opts.clone().const_label("API", "place-text");

        let resp_opts = prometheus::Opts::new("google_maps_api_responses_total", "count of responses from the Google Maps API split by the source");
        let from_cache_opts = resp_opts.clone().const_label("source", "cache");
        let from_remote_opts = resp_opts.const_label("source", "remote");

        GoogleLocFinder {
            client: cache::caching_client(&REDIS.pool),
            api_key: api_key.to_string(),

            geocode_req_counter: metrics::REGISTRY.register_counter("Google Maps API (geocode) requests", geocode_opts),
            text_req_counter:    metrics::REGISTRY.register_counter("Google Maps API (place, text) requests", text_opts),
            cached_resp_counter:  metrics::REGISTRY.register_counter("Google Maps API requests", from_cache_opts),
            fetched_resp_counter: metrics::REGISTRY.register_counter("Google Maps API requests", from_remote_opts),
        }
    }

    pub fn from_env() -> GoogleLocFinder {
        let api_key = std::env::var(FINDER_ENV_API_KEY).expect("Google Maps API key is required!");
        Self::init(api_key.as_str())
    }

    async fn find(&self, address: &str, params: SearchParams<'_>) -> LocResult {
        let mut results = self.find_geo(address, params).await?;
        if results.is_empty() {
            results = self.find_text(address, params).await?;
        }
        Ok(results)
    }

    async fn find_geo(&self, address: &str, params: SearchParams<'_>) -> LocResult {
        self.geocode_req_counter.inc();
        let bounds_part = params.location
            .map(|loc| get_bounds(loc, *SEARCH_RADIUS))
            .map(|(p1, p2)| format!("&bounds={},{}%7C{},{}", p1.0, p1.1, p2.0, p2.1))
            .unwrap_or_default();
        let url = format!("https://maps.googleapis.com/maps/api/geocode/json?key={}&address={}&language={}&region={}{bounds_part}",
                          self.api_key, address, params.lang_code, params.lang_code);
        let resp = self.client.get(url).send().await?;
        self.inc_resp_counter(&resp);

        let json = resp.json::<serde_json::Value>().await?;
        log::info!("Response from Google Maps Geocoding API: {json}");

        let results = iter_over_array(&json["results"])
            .filter_map(map_resp_geo)
            .collect();
        Ok(results)
    }

    async fn find_text(&self, address: &str, params: SearchParams<'_>) -> LocResult {
        self.text_req_counter.inc();
        let resp = self.client.post("https://places.googleapis.com/v1/places:searchText")
            .header(http::header::CONTENT_TYPE.as_str(), mime::APPLICATION_JSON.as_ref())
            .header("X-Goog-Api-Key", &self.api_key)
            .header("X-Goog-FieldMask", "places.displayName,places.formattedAddress,places.location")
            .json(&SearchQuery::new(address, params.lang_code, params.location))
            .send().await?;
        self.inc_resp_counter(&resp);

        let json = resp.json::<serde_json::Value>().await?;
        log::info!("Response from Google Maps Text Search API: {json}");

        let results: Vec<Location> = iter_over_array(&json["places"])
            .filter_map(map_resp_place)
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl LocFinder for GoogleLocFinder {
    async fn find(&self, query: &str, lang_code: &str, location: Option<(f64, f64)>) -> LocResult {
        let params = SearchParams { lang_code, location };
        match *GAPI_MODE {
            GoogleAPIMode::Text => self.find_text(query, params).await,
            GoogleAPIMode::GeoText => self.find(query, params).await,
        }
    }
}

impl WithCachedResponseCounters for GoogleLocFinder {
    fn cached_resp_counter(&self) -> &prometheus::Counter {
        &self.cached_resp_counter
    }

    fn fetched_resp_counter(&self) -> &prometheus::Counter {
        &self.fetched_resp_counter
    }
}

type IterOverJsonArray<'a> = core::iter::FlatMap<
    core::option::IntoIter<&'a Vec<serde_json::Value>>,
    core::slice::Iter<'a, serde_json::Value>,
    fn(&Vec<serde_json::value::Value>) -> core::slice::Iter<serde_json::Value>
>;

fn iter_over_array(v: &serde_json::Value) -> IterOverJsonArray {
    v.as_array().into_iter()
        .flat_map(|x| x.iter())
}

fn map_resp_geo(v: &serde_json::Value) -> Option<Location> {
    let address = Some(v["formatted_address"].as_str()?.to_string());

    let loc = &v["geometry"]["location"];
    let latitude: f64 = loc["lat"].as_f64()?;
    let longitude: f64 = loc["lng"].as_f64()?;

    Some(Location {
        address, latitude, longitude
    })
}

fn map_resp_place(v: &serde_json::Value) -> Option<Location> {
    let name = v["displayName"]["text"].as_str()?.to_string();
    let address = v["formattedAddress"].as_str()?.to_string();
    let full_address = Some(format!("{name}, {address}"));

    let loc = &v["location"];
    let latitude: f64 = loc["latitude"].as_f64()?;
    let longitude: f64 = loc["longitude"].as_f64()?;

    Some(Location {
        address: full_address,
        latitude, longitude
    })
}

fn get_bounds(center: (f64, f64), radius: f64) -> ((f64, f64), (f64, f64)) {
    let (cx, cy) = center;

    let bottom_left = (cx - radius, cy - radius);
    let top_right = (cx + radius, cy + radius);

    (bottom_left, top_right)
}
