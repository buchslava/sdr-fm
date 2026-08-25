use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const FM_MIN_KHZ: u32 = 64_000;
const FM_MAX_KHZ: u32 = 1_080_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Station {
    pub id: String,
    pub name: String,
    pub frequency_khz: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StationsFile {
    #[serde(default)]
    stations: Vec<Station>,
}

fn default_station(id: &str, name: &str, frequency_khz: u32) -> Station {
    Station {
        id: id.to_string(),
        name: name.to_string(),
        frequency_khz,
    }
}

/// Bundled FM presets used when no stations.json exists yet.
pub fn bundled_stations() -> Vec<Station> {
    vec![
        default_station("default-88000", "Радіо Байрактар", 88_000),
        default_station("default-89300", "Радіо Рокс", 89_300),
        default_station("default-90000", "Радіо Релакс", 90_000),
        default_station("default-90400", "Авторадіо", 90_400),
        default_station("default-102000", "Хіт FM", 102_000),
        default_station("default-102400", "Kiss FM", 102_400),
        default_station("default-103000", "Радіо П'ятниця", 103_000),
        default_station("default-103500", "Шлягер FM", 103_500),
        default_station("default-104500", "Наше радіо", 104_500),
        default_station("default-105200", "Люкс FM", 105_200),
        default_station("default-105700", "Power FM", 105_700),
        default_station("default-107000", "Радіо NV", 107_000),
        default_station("default-107400", "Країна FM", 107_400),
        default_station("default-107900", "Мелодія FM", 107_900),
    ]
}

fn sort_stations(stations: &mut [Station]) {
    stations.sort_by_key(|station| station.frequency_khz);
}

fn sorted_stations(stations: Vec<Station>) -> Vec<Station> {
    let mut stations = stations;
    sort_stations(&mut stations);
    stations
}

pub fn config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".sdr-fm"))
}

pub fn stations_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("stations.json"))
}

pub fn ensure_config_dir() -> Option<PathBuf> {
    let dir = config_dir()?;
    let _ = fs::create_dir_all(&dir);
    Some(dir)
}

pub fn load_stations() -> Vec<Station> {
    stations_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|data| serde_json::from_str::<StationsFile>(&data).ok())
        .filter(|file| !file.stations.is_empty())
        .map(|file| sorted_stations(file.stations))
        .unwrap_or_else(bundled_stations)
}

pub fn validate_stations(stations: &[Station]) -> Result<(), String> {
    if stations.is_empty() {
        return Err("At least one station is required.".to_string());
    }

    let mut seen = HashSet::with_capacity(stations.len());
    for station in stations {
        if station.id.trim().is_empty() {
            return Err("Station id cannot be empty.".to_string());
        }

        if !(FM_MIN_KHZ..=FM_MAX_KHZ).contains(&station.frequency_khz) {
            return Err(format!(
                "Frequency must be between {} and {} kHz (FM band).",
                FM_MIN_KHZ, FM_MAX_KHZ
            ));
        }

        if !seen.insert(station.frequency_khz) {
            return Err(format!(
                "Duplicate frequency: {:.1} MHz.",
                station.frequency_khz as f64 / 1000.0
            ));
        }
    }

    Ok(())
}

pub fn save_stations(stations: &[Station]) -> Result<(), String> {
    let stations = sorted_stations(stations.to_vec());
    validate_stations(&stations)?;

    let Some(dir) = ensure_config_dir() else {
        return Err("Home directory not found.".to_string());
    };

    let path = dir.join("stations.json");
    let file = StationsFile { stations };

    let data = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("Failed to serialize stations: {e}"))?;

    fs::write(path, data).map_err(|e| format!("Failed to write stations: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_stations_are_valid() {
        validate_stations(&bundled_stations()).unwrap();
    }

    #[test]
    fn rejects_duplicate_frequency() {
        let stations = vec![
            default_station("a", "A", 101_500),
            default_station("b", "B", 101_500),
        ];
        assert!(validate_stations(&stations).is_err());
    }

    #[test]
    fn sorts_by_frequency() {
        let stations = sorted_stations(vec![
            default_station("high", "High", 101_500),
            default_station("low", "Low", 88_000),
        ]);
        assert_eq!(stations[0].frequency_khz, 88_000);
        assert_eq!(stations[1].frequency_khz, 101_500);
    }
}
