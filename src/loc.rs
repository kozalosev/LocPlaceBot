use crate::metrics::{
    GOOGLE_GEO_REQ_COUNTER,
    GOOGLE_PLACES_REQ_COUNTER,
    GOOGLE_PLACES_TEXT_REQ_COUNTER,
};

const FINDER_ENV_API_KEY: &str = "GOOGLE_MAPS_API_KEY";

#[derive(Debug)]
pub struct Location {
    address: Option<String>,
    latitude: f64,
    longitude: f64
}

pub struct LocFinder {
    api_key: String
}

impl Location {
    pub fn new(latitude: f64, longitude: f64) -> Location {
        Location { address: None, latitude, longitude }
    }

    pub fn address(&self) -> Option<String> {
        self.address.clone()
    }

    pub fn latitude(&self) -> f64 {
        self.latitude
    }

    pub fn longitude(&self) -> f64 {
        self.longitude
    }
}

impl LocFinder {
    pub fn init(api_key: &str) -> LocFinder {
        LocFinder { api_key: api_key.to_string() }
    }

    pub fn from_env() -> LocFinder {
        let api_key = std::env::var(FINDER_ENV_API_KEY).expect("Google Maps API key is required!");
        Self::init(api_key.as_str())
    }

    pub async fn find(&self, address: &str, lang_code: &str) -> Result<Vec<Location>, reqwest::Error> {
        let mut results = self.find_geo(address, lang_code).await?;
        if results.is_empty() {
            results = self.find_place(address, lang_code).await?;
        }
        Ok(results)
    }

    pub async fn find_more(&self, address: &str, lang_code: &str) -> Result<Vec<Location>, reqwest::Error> {
        let mut results = self.find_geo(address, lang_code).await?;
        if results.is_empty() {
            results = self.find_text(address, lang_code).await?;
        }
        Ok(results)
    }

    pub async fn find_geo(&self, address: &str, lang_code: &str) -> Result<Vec<Location>, reqwest::Error> {
        GOOGLE_GEO_REQ_COUNTER.inc();

        let url = format!("https://maps.googleapis.com/maps/api/geocode/json?key={}&address={}&language={}&region={}",
                          self.api_key, address, lang_code, lang_code);
        let resp = reqwest::get(url).await?.json::<serde_json::Value>().await?;

        log::info!("response from Google Maps Geocoding API: {}", resp);

        let results = resp["results"].as_array().unwrap().iter()
            .filter_map(map_resp)
            .collect();
        Ok(results)
    }

    pub async fn find_place(&self, address: &str, lang_code: &str) -> Result<Vec<Location>, reqwest::Error> {
        GOOGLE_PLACES_REQ_COUNTER.inc();

        let url = format!("https://maps.googleapis.com/maps/api/place/findplacefromtext/json?key={}&input={}&inputtype=textquery&language={}&fields=formatted_address,geometry,name",
                          self.api_key, address, lang_code);
        let resp = reqwest::get(url).await?.json::<serde_json::Value>().await?;

        log::info!("response from Google Maps Find Place API: {}", resp);

        let results: Vec<Location> = resp["candidates"].as_array().unwrap().iter()
            .filter_map(map_resp)
            .collect();

        Ok(results)
    }

    pub async fn find_text(&self, address: &str, lang_code: &str) -> Result<Vec<Location>, reqwest::Error> {
        GOOGLE_PLACES_TEXT_REQ_COUNTER.inc();

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

fn map_resp(v: &serde_json::Value) -> Option<Location> {
    let address = Some(v["formatted_address"].as_str()?.to_string());

    let loc = &v["geometry"]["location"];
    let latitude: f64 = loc["lat"].as_f64()?;
    let longitude: f64 = loc["lng"].as_f64()?;

    Some(Location {
        address, latitude, longitude
    })
}
