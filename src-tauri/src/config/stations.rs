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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DefaultCity {
    Kyiv,
    Lviv,
    Kharkiv,
    Odesa,
}

/// City for bundled presets. Override with `SDR_FM_CITY` (`kyiv`, `lviv`, `kharkiv`, `odesa`).
fn parse_default_city(raw: &str) -> DefaultCity {
    match raw.trim().to_ascii_lowercase().as_str() {
        "kyiv" | "kiev" => DefaultCity::Kyiv,
        "lviv" | "lvov" | "lwow" => DefaultCity::Lviv,
        "kharkiv" | "kharkov" => DefaultCity::Kharkiv,
        "odesa" | "odessa" => DefaultCity::Odesa,
        _ => DefaultCity::Kharkiv,
    }
}

fn default_city() -> DefaultCity {
    std::env::var("SDR_FM_CITY")
        .map(|value| parse_default_city(&value))
        .unwrap_or(DefaultCity::Kharkiv)
}

fn default_stations_for_city(city: DefaultCity) -> Vec<Station> {
    // FM networks use different local frequencies per city (radiomap.eu / official station sites).
    match city {
        DefaultCity::Kyiv => vec![
            default_station("default-95200", "Мелодія FM", 95_200),
            default_station("default-96000", "Радіо NV", 96_000),
            default_station("default-96400", "Хіт FM", 96_400),
            default_station("default-98500", "Радіо Байрактар", 98_500),
            default_station("default-100000", "Країна FM", 100_000),
            default_station("default-101100", "Радіо П'ятниця", 101_100),
            default_station("default-101900", "Шлягер FM", 101_900),
            default_station("default-103100", "Люкс FM", 103_100),
            default_station("default-103600", "Радіо Рокс", 103_600),
            default_station("default-104000", "Power FM", 104_000),
            default_station("default-106500", "Kiss FM", 106_500),
            default_station("default-107400", "Авторадіо", 107_400),
            default_station("default-107900", "Наше радіо", 107_900),
        ],
        DefaultCity::Lviv => vec![
            default_station("default-88600", "Радіо NV", 88_600),
            default_station("default-89100", "Радіо Рокс", 89_100),
            default_station("default-90400", "Шлягер FM", 90_400),
            default_station("default-91100", "Kiss FM", 91_100),
            default_station("default-91500", "Мелодія FM", 91_500),
            default_station("default-91900", "Радіо П'ятниця", 91_900),
            default_station("default-93300", "Радіо Релакс", 93_300),
            default_station("default-101300", "Країна FM", 101_300),
            default_station("default-101700", "Хіт FM", 101_700),
            default_station("default-104700", "Люкс FM", 104_700),
            default_station("default-106000", "Наше радіо", 106_000),
            default_station("default-107200", "Радіо Байрактар", 107_200),
        ],
        DefaultCity::Kharkiv => vec![
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
        ],
        DefaultCity::Odesa => vec![
            default_station("default-87900", "Радіо NV", 87_900),
            default_station("default-89000", "Мелодія FM", 89_000),
            default_station("default-90200", "Радіо Рокс", 90_200),
            default_station("default-91000", "Країна FM", 91_000),
            default_station("default-91800", "Шлягер FM", 91_800),
            default_station("default-101000", "Хіт FM", 101_000),
            default_station("default-101400", "Радіо П'ятниця", 101_400),
            default_station("default-101800", "Kiss FM", 101_800),
            default_station("default-102200", "Авторадіо", 102_200),
            default_station("default-104300", "Люкс FM", 104_300),
            default_station("default-104900", "Радіо Байрактар", 104_900),
            default_station("default-107900", "Наше радіо", 107_900),
        ],
    }
}

fn default_stations() -> Vec<Station> {
    default_stations_for_city(default_city())
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
    fn all_city_presets_are_valid() {
        for city in [
            DefaultCity::Kyiv,
            DefaultCity::Lviv,
            DefaultCity::Kharkiv,
            DefaultCity::Odesa,
        ] {
            validate_stations(&default_stations_for_city(city))
                .unwrap_or_else(|err| panic!("{city:?}: {err}"));
        }
    }

    #[test]
    fn parses_default_city_aliases() {
        assert_eq!(parse_default_city("kyiv"), DefaultCity::Kyiv);
        assert_eq!(parse_default_city("Kiev"), DefaultCity::Kyiv);
        assert_eq!(parse_default_city("lviv"), DefaultCity::Lviv);
        assert_eq!(parse_default_city("lvov"), DefaultCity::Lviv);
        assert_eq!(parse_default_city("kharkiv"), DefaultCity::Kharkiv);
        assert_eq!(parse_default_city("odessa"), DefaultCity::Odesa);
        assert_eq!(parse_default_city("unknown"), DefaultCity::Kharkiv);
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
