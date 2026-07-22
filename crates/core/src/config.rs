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
    /// Max HTTP request body size in bytes (WinningCat CSV can be large).
    pub max_body_bytes: usize,
}

impl Config {
    pub fn from_env() -> Self {
        let database_url = resolve_database_url().unwrap_or_else(|| {
            normalize_database_url("postgres://loremetry:loremetry@localhost:5432/loremetry")
        });
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

/// Read Postgres URL from env (Miget may inject `DATABASE_URL`, `POSTGRES_*_URL`, etc.).
pub fn resolve_database_url() -> Option<String> {
    resolve_database_url_with_source().map(|(url, _)| url)
}

/// Same as [`resolve_database_url`], but returns which env var supplied the URL (for logs).
pub fn resolve_database_url_with_source() -> Option<(String, String)> {
    for key in [
        "DATABASE_URL",
        "POSTGRES_DBWEI_URL",
        "POSTGRES_URL",
        "POSTGRESQL_URL",
    ] {
        if let Ok(url) = env::var(key) {
            if let Some(normalized) = normalize_resolved_url(&url) {
                return Some((normalized, key.to_string()));
            }
        }
    }
    // Miget shared/project DB addons often use POSTGRES_<name>_URL (e.g. POSTGRES_DBWEI_URL).
    let mut miget_keys: Vec<String> = env::vars()
        .filter_map(|(key, value)| {
            if key.starts_with("POSTGRES_") && key.ends_with("_URL") && !value.trim().is_empty() {
                Some(key)
            } else {
                None
            }
        })
        .collect();
    miget_keys.sort();
    for key in miget_keys {
        if let Ok(url) = env::var(&key) {
            if let Some(normalized) = normalize_resolved_url(&url) {
                return Some((normalized, key));
            }
        }
    }
    None
}

/// Reject empty values and Miget placeholder IDs that are not connection strings.
fn normalize_resolved_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if !looks_like_postgres_url(raw) {
        return None;
    }
    Some(normalize_database_url(raw))
}

pub fn looks_like_postgres_url(url: &str) -> bool {
    let u = url.trim();
    u.starts_with("postgres://") || u.starts_with("postgresql://")
}

/// Human-readable hint when startup cannot find a valid database URL.
pub fn database_url_diagnostics() -> String {
    let mut lines = vec![
        "Checked DATABASE_URL, POSTGRES_URL, POSTGRESQL_URL, and POSTGRES_*_URL.".to_string(),
    ];
    for key in [
        "DATABASE_URL",
        "POSTGRES_DBWEI_URL",
        "POSTGRES_URL",
        "POSTGRESQL_URL",
    ] {
        match env::var(key) {
            Ok(v) if v.trim().is_empty() => lines.push(format!("{key} is set but empty.")),
            Ok(v) if !looks_like_postgres_url(&v) => {
                lines.push(format!(
                    "{key} is set but does not look like postgres://… (got {} chars; use a full postgres:// connection string at runtime, or attach the Postgres addon to this app).",
                    v.len()
                ));
            }
            Ok(_) => lines.push(format!("{key} looks like a postgres URL.")),
            Err(_) => lines.push(format!("{key} is not set.")),
        }
    }
    let mut miget_keys: Vec<String> = env::vars()
        .filter_map(|(key, value)| {
            if key.starts_with("POSTGRES_") && key.ends_with("_URL") && !value.trim().is_empty() {
                Some(key)
            } else {
                None
            }
        })
        .collect();
    miget_keys.sort();
    for key in miget_keys {
        if matches!(
            key.as_str(),
            "POSTGRES_DBWEI_URL" | "POSTGRES_URL" | "POSTGRESQL_URL"
        ) {
            continue;
        }
        match env::var(&key) {
            Ok(v) if looks_like_postgres_url(&v) => {
                lines.push(format!("{key} looks like a postgres URL."));
            }
            Ok(v) => lines.push(format!(
                "{key} is set but does not look like postgres://… ({} chars).",
                v.len()
            )),
            Err(_) => {}
        }
    }
    lines.join(" ")
}

/// Host portion of the URL for startup logs (no credentials).
pub fn database_url_host(url: &str) -> String {
    url.split('@')
        .nth(1)
        .unwrap_or(url)
        .split('?')
        .next()
        .unwrap_or(url)
        .to_string()
}

/// Prepare a Miget/Heroku-style URL for sqlx (rustls).
pub fn normalize_database_url(url: &str) -> String {
    let mut out = url.trim().replace("postgres://", "postgresql://");
    if !out.contains("sslmode=") {
        let sslmode = env::var("DATABASE_SSLMODE").unwrap_or_else(|_| {
            if is_local_db_host(&out) {
                "disable".into()
            } else {
                "prefer".into()
            }
        });
        let sep = if out.contains('?') { '&' } else { '?' };
        out.push_str(&format!("{sep}sslmode={sslmode}"));
    }
    out
}

fn is_local_db_host(url: &str) -> bool {
    let host = database_url_host(url).to_lowercase();
    host.starts_with("localhost")
        || host.starts_with("127.0.0.1")
        || host.starts_with("[::1]")
        || host.starts_with("host.docker.internal")
}

#[cfg(test)]
mod tests {
    use super::normalize_database_url;

    #[test]
    fn normalizes_scheme_and_sslmode() {
        let u = normalize_database_url("postgres://user:pass@host:5432/db");
        assert!(u.starts_with("postgresql://"));
        assert!(u.contains("sslmode=prefer"));
    }

    #[test]
    fn local_host_uses_disable_sslmode() {
        let u = normalize_database_url("postgres://user:pass@localhost:5432/db");
        assert!(u.contains("sslmode=disable"));
    }
}
