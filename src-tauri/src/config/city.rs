use serde::{Deserialize, Serialize};

/// Ukrainian cities with bundled FM frequency tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum City {
    Kyiv,
    Lviv,
    Kharkiv,
    Odesa,
}

impl City {
    pub const ALL: [City; 4] = [City::Kyiv, City::Lviv, City::Kharkiv, City::Odesa];

    pub fn id(self) -> &'static str {
        match self {
            City::Kyiv => "kyiv",
            City::Lviv => "lviv",
            City::Kharkiv => "kharkiv",
            City::Odesa => "odesa",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            City::Kyiv => "Київ",
            City::Lviv => "Львів",
            City::Kharkiv => "Харків",
            City::Odesa => "Одеса",
        }
    }

    /// Approximate city center for nearest-city detection.
    pub fn coords(self) -> (f64, f64) {
        match self {
            City::Kyiv => (50.4501, 30.5234),
            City::Lviv => (49.8397, 24.0297),
            City::Kharkiv => (49.9935, 36.2304),
            City::Odesa => (46.4825, 30.7233),
        }
    }

    pub fn parse(raw: &str) -> Option<City> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "kyiv" | "kiev" => Some(City::Kyiv),
            "lviv" | "lvov" | "lwow" => Some(City::Lviv),
            "kharkiv" | "kharkov" => Some(City::Kharkiv),
            "odesa" | "odessa" => Some(City::Odesa),
            _ => None,
        }
    }
}

/// Great-circle distance in km (Haversine).
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_KM: f64 = 6371.0;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    EARTH_KM * c
}

pub fn nearest_city(lat: f64, lon: f64) -> City {
    City::ALL
        .into_iter()
        .min_by(|a, b| {
            let (alat, alon) = a.coords();
            let (blat, blon) = b.coords();
            let da = haversine_km(lat, lon, alat, alon);
            let db = haversine_km(lat, lon, blat, blon);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(City::Kharkiv)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CityInfo {
    pub id: String,
    pub name: String,
}

pub fn list_cities() -> Vec<CityInfo> {
    City::ALL
        .into_iter()
        .map(|city| CityInfo {
            id: city.id().to_string(),
            name: city.display_name().to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_city_aliases() {
        assert_eq!(City::parse("kyiv"), Some(City::Kyiv));
        assert_eq!(City::parse("Kiev"), Some(City::Kyiv));
        assert_eq!(City::parse("lviv"), Some(City::Lviv));
        assert_eq!(City::parse("lvov"), Some(City::Lviv));
        assert_eq!(City::parse("kharkiv"), Some(City::Kharkiv));
        assert_eq!(City::parse("odessa"), Some(City::Odesa));
        assert_eq!(City::parse("unknown"), None);
    }

    #[test]
    fn nearest_city_picks_kharkiv() {
        assert_eq!(nearest_city(49.99, 36.23), City::Kharkiv);
    }

    #[test]
    fn nearest_city_picks_lviv() {
        assert_eq!(nearest_city(49.84, 24.03), City::Lviv);
    }

    #[test]
    fn nearest_city_picks_kyiv() {
        assert_eq!(nearest_city(50.45, 30.52), City::Kyiv);
    }

    #[test]
    fn nearest_city_picks_odesa() {
        assert_eq!(nearest_city(46.48, 30.73), City::Odesa);
    }
}
