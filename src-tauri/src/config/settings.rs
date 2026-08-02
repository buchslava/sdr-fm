use std::fs;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::city::{City, nearest_city};
use super::stations::ensure_config_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SettingsFile {
    #[serde(default = "default_city_id")]
    city: String,
}

fn default_city_id() -> String {
    City::Kharkiv.id().to_string()
}

pub fn settings_path() -> Option<std::path::PathBuf> {
    super::stations::config_dir().map(|d| d.join("settings.json"))
}

fn env_city() -> Option<City> {
    std::env::var("SDR_FM_CITY")
        .ok()
        .and_then(|value| City::parse(&value))
}

/// Resolve the active city: env override → settings.json → IP detect → Kharkiv.
pub fn resolve_city() -> City {
    if let Some(city) = env_city() {
        return city;
    }

    if let Some(city) = load_saved_city() {
        return city;
    }

    let city = detect_city_from_ip().unwrap_or(City::Kharkiv);
    let _ = save_city(city);
    city
}

pub fn current_city() -> City {
    if let Some(city) = env_city() {
        return city;
    }
    load_saved_city().unwrap_or(City::Kharkiv)
}

fn load_saved_city() -> Option<City> {
    let path = settings_path()?;
    let data = fs::read_to_string(path).ok()?;
    let parsed: SettingsFile = serde_json::from_str(&data).ok()?;
    City::parse(&parsed.city)
}

pub fn save_city(city: City) -> Result<(), String> {
    let Some(dir) = ensure_config_dir() else {
        return Err("Home directory not found.".to_string());
    };

    let path = dir.join("settings.json");
    let file = SettingsFile {
        city: city.id().to_string(),
    };
    let data = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;
    fs::write(path, data).map_err(|e| format!("Failed to write settings: {e}"))
}

#[derive(Debug, Deserialize)]
struct IpApiResponse {
    status: String,
    #[serde(default)]
    lat: Option<f64>,
    #[serde(default)]
    lon: Option<f64>,
}

/// Best-effort IP geolocation; returns None on network/parse failure.
pub fn detect_city_from_ip() -> Option<City> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(3))
        .timeout_read(Duration::from_secs(5))
        .build();

    let response = agent
        .get("http://ip-api.com/json/?fields=status,lat,lon")
        .call()
        .ok()?;

    let body: IpApiResponse = response.into_json().ok()?;
    if body.status != "success" {
        return None;
    }

    let lat = body.lat?;
    let lon = body.lon?;
    Some(nearest_city(lat, lon))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_file_round_trip_shape() {
        let file = SettingsFile {
            city: City::Kharkiv.id().to_string(),
        };
        let json = serde_json::to_string(&file).unwrap();
        let parsed: SettingsFile = serde_json::from_str(&json).unwrap();
        assert_eq!(City::parse(&parsed.city), Some(City::Kharkiv));
    }
}
