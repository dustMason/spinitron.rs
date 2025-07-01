use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::models::Show;

#[derive(Debug)]
pub struct AppConfig {
    pub stations: HashMap<String, StationConfig>,
}

#[derive(Debug)]
pub struct StationConfig {
    pub ignore_patterns: Vec<Regex>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StationConfigRaw {
    #[serde(default)]
    pub ignores: Vec<String>,
}

impl StationConfig {
    pub fn new(ignores: Vec<String>) -> Result<Self> {
        let mut ignore_patterns = Vec::new();

        for pattern in &ignores {
            match Regex::new(pattern) {
                Ok(regex) => ignore_patterns.push(regex),
                Err(e) => {
                    eprintln!("Warning: Invalid regex pattern '{}': {}", pattern, e);
                }
            }
        }

        Ok(StationConfig { ignore_patterns })
    }

    pub fn filter_shows(&self, shows: Vec<Show>) -> Vec<Show> {
        if !self.ignore_patterns.is_empty() {
            shows
                .into_iter()
                .filter(|show| {
                    !self
                        .ignore_patterns
                        .iter()
                        .any(|regex| regex.is_match(&show.title))
                })
                .collect()
        } else {
            shows
        }
    }
}

impl AppConfig {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let raw_config: HashMap<String, HashMap<String, StationConfigRaw>> =
            toml::from_str(&content)?;

        let mut stations = HashMap::new();

        if let Some(stations_raw) = raw_config.get("stations") {
            for (station_name, station_raw) in stations_raw {
                match StationConfig::new(station_raw.ignores.clone()) {
                    Ok(station_config) => {
                        stations.insert(station_name.clone(), station_config);
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to create config for station {}: {}",
                            station_name, e
                        );
                    }
                }
            }
        }

        Ok(AppConfig { stations })
    }
}
