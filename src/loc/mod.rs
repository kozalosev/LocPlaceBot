use std::collections::HashMap;
use async_trait::async_trait;

pub mod google;
pub mod yandex;
pub mod osm;

#[cfg(test)]
mod test;

#[derive(Debug, Clone)]
pub struct Location {
    address: Option<String>,
    latitude: f64,
    longitude: f64
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

pub type LocResult = Result<Vec<Location>, anyhow::Error>;
pub type DynLocFinder = Box<dyn LocFinder + Sync + Send>;

#[async_trait]
pub trait LocFinder {
    async fn find(&self, query: &str, lang_code: &str) -> LocResult;
}

pub struct SearchChain {
    global_finders: Vec<DynLocFinder>,
    regional_finders: HashMap<String, Vec<DynLocFinder>>,
}

impl SearchChain {
    pub fn new(global_finders: Vec<DynLocFinder>) -> SearchChain {
        SearchChain {
            global_finders,
            regional_finders: HashMap::new()
        }
    }

    /// Reserved for Yandex Maps or 2GIS providers which may be used for RU locale in the future
    #[allow(dead_code)]
    pub fn for_lang_code(mut self, lc: &str, mut finders: Vec<DynLocFinder>) -> Self {
        self.regional_finders
            .entry(lc.to_string())
            .or_insert(Vec::with_capacity(finders.len()))
            .append(&mut finders);
        self
    }

    pub async fn find(&self, query: &str, lang_code: &str) -> Vec<Location> {
        let futures = self.regional_finders.get(lang_code)
            .unwrap_or(&self.global_finders)
            .iter()
            .map(|f| f.find(query, lang_code));

        for fut in futures {
            match fut.await {
                Ok(res) if res.len() > 0 => return res,
                Ok(_) => continue,
                Err(err) => log::error!("couldn't fetch loc data: {err}"),
            }
        };

        Vec::default()
    }
}
