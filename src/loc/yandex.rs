use std::str::FromStr;
use anyhow::anyhow;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use reqwest_middleware::ClientWithMiddleware;
use strum_macros::EnumString;
use super::cache::WithCachedResponseCounters;
use super::{cache, Location, LocFinder, LocResult};
use crate::metrics;

const GEOCODER_ENV_API_KEY: &str = "YANDEX_MAPS_GEOCODER_API_KEY";
const PLACES_ENV_API_KEY: &str   = "YANDEX_MAPS_PLACES_API_KEY";

pub static YAPI_MODE: Lazy<YandexAPIMode> = Lazy::new(|| {
    let val = std::env::var("YAPI_MODE").expect("YAPI_MODE must be set!");
    log::info!("YAPI_MODE is {val}");
    YandexAPIMode::from_str(val.as_str()).expect("Invalid value of YAPI_MODE")
});

#[derive(EnumString)]
pub enum YandexAPIMode {
    Geocode,    // HTTP Geocoder request
    Place,      // Places API request
    GeoPlace,   // Geocoder request first, Places if nothing was found
}

/// Load and check required parameters at startup
pub fn preload_env_vars() {
    let _ = *YAPI_MODE;
}

pub struct YandexLocFinder {
    client: ClientWithMiddleware,

    geocode_api_key: String,
    places_api_key: Option<String>,

    geocode_req_counter: prometheus::Counter,
    place_req_counter: prometheus::Counter,
    cached_resp_counter: prometheus::Counter,
    fetched_resp_counter: prometheus::Counter,
}

impl YandexLocFinder {
    pub fn init(geocode_api_key: String, places_api_key: Option<String>) -> YandexLocFinder {
        let base_opts = prometheus::Opts::new("yandex_maps_api_requests_total", "count of requests to the Yandex Maps API");
        let geocode_opts = base_opts.clone().const_label("API", "geocode");
        let place_opts   = base_opts.clone().const_label("API", "place");

        let resp_opts = prometheus::Opts::new("yandex_maps_api_responses_total", "count of responses from the Yandex Maps API split by the source");
        let from_cache_opts = resp_opts.clone().const_label("source", "cache");
        let from_remote_opts = resp_opts.const_label("source", "remote");

        YandexLocFinder {
            client: cache::caching_client(),

            geocode_api_key,
            places_api_key,

            geocode_req_counter:  metrics::REGISTRY.register_counter("Yandex Maps API (geocode) requests", geocode_opts),
            place_req_counter:    metrics::REGISTRY.register_counter("Yandex Maps API (place) requests", place_opts),
            cached_resp_counter:  metrics::REGISTRY.register_counter("Yandex Maps API requests", from_cache_opts),
            fetched_resp_counter: metrics::REGISTRY.register_counter("Yandex Maps API requests", from_remote_opts),
        }
    }

    pub fn from_env() -> YandexLocFinder {
        let geocode_api_key = std::env::var(GEOCODER_ENV_API_KEY).expect("Yandex Maps Geocoder API key is required!");
        let places_api_key = match *YAPI_MODE {
            YandexAPIMode::Place | YandexAPIMode::GeoPlace => {
                let api_key = std::env::var(PLACES_ENV_API_KEY).expect("Yandex Maps Places API key is required!");
                Some(api_key)
            }
            YandexAPIMode::Geocode => None
        };
        Self::init(geocode_api_key, places_api_key)
    }

    pub async fn find_geo_place(&self, address: &str, lang_code: &str) -> LocResult {
        let mut results = self.find_geo(address, lang_code).await?;
        if results.is_empty() {
            results = self.find_place(address, lang_code).await?;
        }
        Ok(results)
    }

    pub async fn find_geo(&self, address: &str, lang_code: &str) -> LocResult {
        self.geocode_req_counter.inc();

        let url = format!("https://geocode-maps.yandex.ru/1.x?apikey={}&lang={}&geocode={}&format=json",
                          self.geocode_api_key, lang_code, address);
        let resp = self.client.get(url).send().await?;
        self.inc_resp_counter(&resp);

        let json = resp.json::<serde_json::Value>().await?;
        log::info!("response from Yandex Maps Geocoder: {json}");

        let empty: Vec<serde_json::Value> = Vec::new();
        let result = json["response"]["GeoObjectCollection"]["featureMember"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .filter_map(geocode_elem_mapper)
            .collect();
        Ok(result)
    }

    pub async fn find_place(&self, address: &str, lang_code: &str) -> LocResult {
        self.place_req_counter.inc();

        let api_key = self.places_api_key.clone()
            .ok_or(anyhow!("unexpected absence of a key for Yandex Maps Places API"))?;

        let url = format!("https://search-maps.yandex.ru/v1/?apikey={}&lang={}&text={}",
                          api_key, lang_code, address);
        let resp = self.client.get(url).send().await?;
        self.inc_resp_counter(&resp);

        let json = resp.json::<serde_json::Value>().await?;
        log::info!("response from Yandex Maps Places API: {json}");

        let empty: Vec<serde_json::Value> = Vec::new();
        let result = json["features"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .filter_map(places_elem_mapper)
            .collect();
        Ok(result)
    }
}

#[async_trait]
impl LocFinder for YandexLocFinder {
    async fn find(&self, query: &str, lang_code: &str) -> LocResult {
        match *YAPI_MODE {
            YandexAPIMode::Geocode => self.find_geo(query, lang_code).await,
            YandexAPIMode::Place => self.find_place(query, lang_code).await,
            YandexAPIMode::GeoPlace => self.find_geo_place(query, lang_code).await,
        }
    }
}

impl WithCachedResponseCounters for YandexLocFinder {
    fn cached_resp_counter(&self) -> &prometheus::Counter {
        &self.cached_resp_counter
    }

    fn fetched_resp_counter(&self) -> &prometheus::Counter {
        &self.fetched_resp_counter
    }
}

fn geocode_elem_mapper(v: &serde_json::Value) -> Option<Location> {
    let obj = &v["GeoObject"];
    let metadata = &obj["metaDataProperty"]["GeocoderMetaData"];
    let address = Some(metadata["text"].as_str()?.to_string());

    let pos = &obj["Point"]["pos"].as_str()?
        .split(' ')
        .collect::<Vec<&str>>();
    if pos.len() < 2 {
        log::error!("pos length < 2: {pos:?}");
        return None
    }
    let longitude: f64 = pos[0].parse().ok()?;
    let latitude: f64 = pos[1].parse().ok()?;

    Some(Location {
        address, latitude, longitude
    })
}

fn places_elem_mapper(v: &serde_json::Value) -> Option<Location> {
    let name = v["properties"]["name"].as_str()?;
    let description = v["properties"]["description"].as_str()?;
    let address = Some(format!("{}, {}", name, description));

    let loc = &v["geometry"]["coordinates"];
    let longitude: f64 = loc[0].as_f64()?;
    let latitude: f64 = loc[1].as_f64()?;

    Some(Location {
        address, latitude, longitude
    })
}
