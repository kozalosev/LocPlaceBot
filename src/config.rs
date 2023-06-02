use std::str::FromStr;
use once_cell::sync::Lazy;
use strum_macros::EnumString;

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
pub fn init() {
    let _ = *GAPI_MODE;
}
