//! Server-side secrets and runtime config from environment variables.

use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub anthropic_api_key: String,
    pub tokenmix_api_key: String,
    pub canopy_api_key: String,
    pub dataforseo_login: String,
    pub dataforseo_password: String,
    pub default_provider: String,
    pub static_dir: PathBuf,
    /// Shared secret for `/api/admin/*`. Empty disables admin endpoints.
    pub admin_token: String,
    /// Max HTTP request body size in bytes (WinningCat CSV can be large).
    pub max_body_bytes: usize,
}

impl Config {
    pub fn from_env() -> Self {
        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://loremetry:loremetry@localhost:5432/loremetry".into());
        let port = env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);
        let static_dir = env::var("STATIC_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./ui/dist"));

        Self {
            database_url,
            port,
            anthropic_api_key: env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
            tokenmix_api_key: env::var("TOKENMIX_API_KEY").unwrap_or_default(),
            canopy_api_key: env::var("CANOPY_API_KEY").unwrap_or_default(),
            dataforseo_login: env::var("DATAFORSEO_LOGIN").unwrap_or_default(),
            dataforseo_password: env::var("DATAFORSEO_PASSWORD").unwrap_or_default(),
            default_provider: env::var("DEFAULT_PROVIDER").unwrap_or_else(|_| "claude".into()),
            static_dir,
            admin_token: env::var("ADMIN_TOKEN").unwrap_or_default(),
            max_body_bytes: env::var("MAX_BODY_MB")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .map(|mb| mb * 1024 * 1024)
                .unwrap_or(256 * 1024 * 1024),
        }
    }

    /// Resolve API key for a provider. Request override wins if non-empty (BYOK), else env.
    pub fn resolve_api_key(&self, provider: &str, request_key: &str) -> String {
        if !request_key.trim().is_empty() {
            return request_key.to_string();
        }
        match provider {
            "tokenmix" => self.tokenmix_api_key.clone(),
            _ => self.anthropic_api_key.clone(),
        }
    }

    pub fn resolve_canopy_key(&self, request_key: &str) -> String {
        if !request_key.trim().is_empty() {
            return request_key.to_string();
        }
        self.canopy_api_key.clone()
    }

    pub fn resolve_dataforseo(&self, login: &str, password: &str) -> (String, String) {
        let l = if login.trim().is_empty() {
            self.dataforseo_login.clone()
        } else {
            login.to_string()
        };
        let p = if password.trim().is_empty() {
            self.dataforseo_password.clone()
        } else {
            password.to_string()
        };
        (l, p)
    }
}
