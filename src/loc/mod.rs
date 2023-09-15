use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;

pub mod google;
pub mod yandex;
pub mod osm;
pub mod cache;

#[cfg(test)]
mod test;

const DISABLE_ENV_PREFIX: &str = "DISABLE_FINDER_";

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
pub type DynLocFinder = Arc<dyn LocFinder>;

#[async_trait]
pub trait LocFinder : Sync + Send {
    async fn find(&self, query: &str, lang_code: &str) -> LocResult;
}

pub struct SearchChain {
    global_finders: Vec<DynLocFinder>,
    regional_finders: HashMap<String, Vec<DynLocFinder>>,
}

impl SearchChain {
    pub fn new(global_finders: Vec<LocFinderChainWrapper>) -> SearchChain {
        let global_finders = global_finders.into_iter()
            .filter_map(LocFinderChainWrapper::unwrap_if_not_disabled)
            .collect();
        SearchChain {
            global_finders,
            regional_finders: HashMap::new()
        }
    }

    pub fn for_lang_code(mut self, lc: &str, finders: Vec<LocFinderChainWrapper>) -> Self {
        let mut finders = finders.into_iter()
            .filter_map(LocFinderChainWrapper::unwrap_if_not_disabled)
            .collect::<Vec<DynLocFinder>>();
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

pub fn finder(env: &str, instance: impl LocFinder + 'static) -> LocFinderChainWrapper {
    LocFinderChainWrapper::wrap(env, Arc::new(instance))
}

#[derive(Clone)]
pub struct LocFinderChainWrapper {
    env_suffix: String,
    finder: DynLocFinder
}

impl LocFinderChainWrapper {
    pub fn wrap(env_suffix: &str, finder: DynLocFinder) -> Self {
        LocFinderChainWrapper {
            env_suffix: env_suffix.to_owned(),
            finder
        }
    }

    fn unwrap_if_not_disabled(self) -> Option<DynLocFinder> {
        let disabled = std::env::var(DISABLE_ENV_PREFIX.to_owned() + self.env_suffix.as_str())
            .map(|v| v == "true" || v == "1" || v == "yes" || v == "y")
            .unwrap_or(false);
        if disabled {
            log::warn!("The {} finder is disabled!", self.env_suffix);
            None
        } else {
            Some(self.finder)
        }
    }
}
