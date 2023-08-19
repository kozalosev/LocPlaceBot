use std::str::FromStr;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use strum_macros::EnumString;
use crate::loc::{Location, LocFinder, LocResult};
use crate::metrics;

const FINDER_ENV_API_KEY: &str = "GOOGLE_MAPS_API_KEY";

pub static GAPI_MODE: Lazy<GoogleAPIMode> = Lazy::new(|| {
    let val = std::env::var("GAPI_MODE").expect("GAPI_MODE must be set!");
    log::info!("GAPI_MODE is {val}");
    GoogleAPIMode::from_str(val.as_str()).expect("Invalid value of GAPI_MODE")
});

#[derive(EnumString)]
pub enum GoogleAPIMode {
    Place,      // Find Place request
    Text,       // Text Search request
    GeoPlace,   // Geocoding request first, Find Place if ZERO_RESULTS
    GeoText,    // Geocoding request first, Text Search if ZERO_RESULTS
}

/// Load and check required parameters at startup
pub fn preload_env_vars() {
    let _ = *GAPI_MODE;
}

pub struct GoogleLocFinder {
    api_key: String,

    geocode_req_counter: prometheus::Counter,
    place_req_counter: prometheus::Counter,
    text_req_counter: prometheus::Counter,
}

impl GoogleLocFinder {
    pub fn init(api_key: &str) -> GoogleLocFinder {
        let base_opts = prometheus::Opts::new("google_maps_api_requests_total", "count of requests to the Google Maps API");
        let geocode_opts = base_opts.clone().const_label("API", "geocode");
        let place_opts   = base_opts.clone().const_label("API", "place");
        let text_opts    = base_opts.clone().const_label("API", "place-text");

        GoogleLocFinder {
            api_key: api_key.to_string(),

            geocode_req_counter: metrics::REGISTRY.register_counter("Google Maps API (geocode) requests", geocode_opts),
            place_req_counter:   metrics::REGISTRY.register_counter("Google Maps API (place) requests", place_opts),
            text_req_counter:    metrics::REGISTRY.register_counter("Google Maps API (place, text) requests", text_opts),
        }
    }

    pub fn from_env() -> GoogleLocFinder {
        let api_key = std::env::var(FINDER_ENV_API_KEY).expect("Google Maps API key is required!");
        Self::init(api_key.as_str())
    }

    pub async fn find(&self, address: &str, lang_code: &str) -> LocResult {
        let mut results = self.find_geo(address, lang_code).await?;
        if results.is_empty() {
            results = self.find_place(address, lang_code).await?;
        }
        Ok(results)
    }

    pub async fn find_more(&self, address: &str, lang_code: &str) -> LocResult {
        let mut results = self.find_geo(address, lang_code).await?;
        if results.is_empty() {
            results = self.find_text(address, lang_code).await?;
        }
        Ok(results)
    }

    pub async fn find_geo(&self, address: &str, lang_code: &str) -> LocResult {
        self.geocode_req_counter.inc();

        let url = format!("https://maps.googleapis.com/maps/api/geocode/json?key={}&address={}&language={}&region={}",
                          self.api_key, address, lang_code, lang_code);
        let resp = reqwest::get(url).await?.json::<serde_json::Value>().await?;

        log::info!("response from Google Maps Geocoding API: {}", resp);

        let results = resp["results"].as_array().unwrap().iter()
            .filter_map(map_resp)
            .collect();
        Ok(results)
    }

    pub async fn find_place(&self, address: &str, lang_code: &str) -> LocResult {
        self.place_req_counter.inc();

        let url = format!("https://maps.googleapis.com/maps/api/place/findplacefromtext/json?key={}&input={}&inputtype=textquery&language={}&fields=formatted_address,geometry,name",
                          self.api_key, address, lang_code);
        let resp = reqwest::get(url).await?.json::<serde_json::Value>().await?;

        log::info!("response from Google Maps Find Place API: {}", resp);

        let results: Vec<Location> = resp["candidates"].as_array().unwrap().iter()
            .filter_map(map_resp)
            .collect();

        Ok(results)
    }

    pub async fn find_text(&self, address: &str, lang_code: &str) -> LocResult {
        self.text_req_counter.inc();

        let url = format!("https://maps.googleapis.com/maps/api/place/textsearch/json?key={}&query={}&language={}&region={}",
                          self.api_key, address, lang_code, lang_code);
        let resp = reqwest::get(url).await?.json::<serde_json::Value>().await?;

        log::info!("response from Google Maps Text Search API: {}", resp);

        let results: Vec<Location> = resp["results"].as_array().unwrap().iter()
            .filter_map(map_resp)
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl LocFinder for GoogleLocFinder {
    async fn find(&self, query: &str, lang_code: &str) -> LocResult {
        match *GAPI_MODE {
            GoogleAPIMode::Place => self.find_place(query, lang_code).await,
            GoogleAPIMode::Text => self.find_text(query, lang_code).await,
            GoogleAPIMode::GeoPlace => self.find(query, lang_code).await,
            GoogleAPIMode::GeoText => self.find_more(query, lang_code).await,
        }
    }
}

fn map_resp(v: &serde_json::Value) -> Option<Location> {
    let address = Some(v["formatted_address"].as_str()?.to_string());

    let loc = &v["geometry"]["location"];
    let latitude: f64 = loc["lat"].as_f64()?;
    let longitude: f64 = loc["lng"].as_f64()?;

    Some(Location {
        address, latitude, longitude
    })
}
