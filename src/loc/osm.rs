use async_trait::async_trait;
use reqwest::header::{ACCEPT_LANGUAGE, USER_AGENT};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use http_cache_reqwest::{Cache, CacheMode, MokaManager, HttpCache, CacheOptions};
use super::{LocFinder, LocResult, Location};

pub struct OpenStreetMapLocFinder {
    client: ClientWithMiddleware
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

        OpenStreetMapLocFinder { client }
    }
}

#[async_trait]
impl LocFinder for OpenStreetMapLocFinder {
    async fn find(&self, query: &str, lang_code: &str) -> LocResult {
        let url = format!("https://nominatim.openstreetmap.org/search?q={query}&format=json");
        let resp = self.client.get(url)
            .header(USER_AGENT, "kozalosev/LocPlaceBot")
            .header(ACCEPT_LANGUAGE, lang_code)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        log::info!("response from Open Street Map Nominatim API: {}", resp);

        let results = resp.as_array().unwrap().iter()
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