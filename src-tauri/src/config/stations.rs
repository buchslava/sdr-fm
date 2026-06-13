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

fn default_stations() -> Vec<Station> {
    vec![
        default_station("default-88000", "BBC Radio 2", 88_000),
        default_station("default-89100", "NPR / public radio", 89_100),
        default_station("default-90000", "Classic FM", 90_000),
        default_station("default-91500", "KJZZ / classical", 91_500),
        default_station("default-93900", "WNYC", 93_900),
        default_station("default-95500", "KLOS", 95_500),
        default_station("default-97100", "KROQ", 97_100),
        default_station("default-98500", "WBZ-FM", 98_500),
        default_station("default-100300", "WHTZ (Z100)", 100_300),
        default_station("default-101500", "WXXL", 101_500),
        default_station("default-102700", "WNEW-FM", 102_700),
        default_station("default-104300", "WAXQ", 104_300),
        default_station("default-106700", "WLTW (Lite FM)", 106_700),
        default_station("default-107900", "Band edge", 107_900),
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
    let path = match stations_path() {
        Some(p) => p,
        None => return default_stations(),
    };

    let Ok(data) = fs::read_to_string(&path) else {
        return default_stations();
    };

    let parsed: StationsFile = match serde_json::from_str(&data) {
        Ok(file) => file,
        Err(_) => return default_stations(),
    };

    if parsed.stations.is_empty() {
        return default_stations();
    }

    sorted_stations(parsed.stations)
}

pub fn validate_stations(stations: &[Station]) -> Result<(), String> {
    if stations.is_empty() {
        return Err("At least one station is required.".to_string());
    }

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
    }

    for (i, a) in stations.iter().enumerate() {
        for b in stations.iter().skip(i + 1) {
            if a.frequency_khz == b.frequency_khz {
                return Err(format!(
                    "Duplicate frequency: {:.1} MHz.",
                    a.frequency_khz as f64 / 1000.0
                ));
            }
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
    let file = StationsFile {
        stations,
    };

    let data = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("Failed to serialize stations: {e}"))?;

    fs::write(path, data).map_err(|e| format!("Failed to write stations: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_stations_are_valid() {
        validate_stations(&default_stations()).unwrap();
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
