//! Configuration module

use std::env;

pub struct Config {
    pub host: String,
    pub port: u16,
    pub api_key: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            api_key: env::var("API_KEY").ok(),
        }
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
