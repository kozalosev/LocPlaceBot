use std::str::FromStr;
use once_cell::sync::Lazy;

const ENV_QUERY_CHECK_MODE: &str = "QUERY_CHECK_MODE";

#[derive(strum_macros::EnumString, Default, PartialEq, Eq)]
#[strum(ascii_case_insensitive)]
pub enum QueryCheckMode {
    #[default]
    Emptiness,
    Regex,
}

impl QueryCheckMode {
    pub fn load_from_env() -> Self {
        std::env::var(ENV_QUERY_CHECK_MODE).ok()
            .and_then(|v| QueryCheckMode::from_str(&v)
                .inspect_err(|err| log::error!("could not parse query check mode: {}", err))
                .ok())
            .unwrap_or_default()
    }
}

pub static QUERY_CHECK_MODE: Lazy<QueryCheckMode> = Lazy::new(
    if cfg!(test) {
        || QueryCheckMode::Regex
    } else {
        QueryCheckMode::load_from_env
    }
);

pub fn preload_env_vars() {
    let _ = *QUERY_CHECK_MODE;
}
