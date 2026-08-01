//! Platform credentials loaded from environment variables at startup.

use std::sync::Arc;
use tokio::sync::RwLock;

use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone, Debug, Default)]
pub struct PlatformCredentials {
    pub anthropic_api_key: String,
    pub tokenmix_api_key: String,
    pub canopy_api_key: String,
    pub dataforseo_login: String,
    pub dataforseo_password: String,
    pub default_provider: String,
    pub clerk_publishable_key: String,
    pub clerk_jwt_issuer: String,
    pub bootstrap_admin_email: String,
    pub admin_bypass_token: String,
}

#[derive(Clone)]
pub struct PlatformSecrets {
    inner: Arc<RwLock<PlatformCredentials>>,
}

impl PlatformSecrets {
    /// Load all credentials from environment variables. Synchronous — no DB needed.
    pub fn load_from_env() -> Self {
        let anthropic_api_key = env_or_empty("anthropic_api_key");
        let tokenmix_api_key = env_or_empty("tokenmix_api_key");
        let canopy_api_key = env_or_empty("canopy_api_key");
        let dataforseo_login = env_or_empty("dataforseo_login");
        let dataforseo_password = env_or_empty("dataforseo_password");
        let default_provider = env_or_default("default_provider", "tokenmix");
        let clerk_publishable_key = env_or_empty("clerk_publishable_key");
        let clerk_jwt_issuer = env_or_empty("clerk_jwt_issuer");
        let admin_bypass_token = env_or_empty("admin_bypass_token");
        let bootstrap_admin_email =
            normalize_bootstrap_email(&env_or_empty("bootstrap_admin_email"));

        let creds = PlatformCredentials {
            anthropic_api_key,
            tokenmix_api_key,
            canopy_api_key,
            dataforseo_login,
            dataforseo_password,
            default_provider,
            clerk_publishable_key,
            clerk_jwt_issuer,
            bootstrap_admin_email,
            admin_bypass_token,
        };

        log_configured_services(&creds);

        Self {
            inner: Arc::new(RwLock::new(creds)),
        }
    }

    pub async fn get(&self) -> PlatformCredentials {
        self.inner.read().await.clone()
    }

    pub async fn resolve_api_key(&self, provider: &str) -> String {
        let c = self.inner.read().await;
        match provider {
            "tokenmix" => c.tokenmix_api_key.clone(),
            _ => c.anthropic_api_key.clone(),
        }
    }

    pub async fn canopy_key(&self) -> String {
        self.inner.read().await.canopy_api_key.clone()
    }

    pub async fn dataforseo(&self) -> (String, String) {
        let c = self.inner.read().await;
        (
            c.dataforseo_login.trim().to_string(),
            c.dataforseo_password.trim().to_string(),
        )
    }

    pub async fn default_provider(&self) -> String {
        let c = self.inner.read().await;
        if c.default_provider.trim().is_empty() {
            "tokenmix".into()
        } else {
            c.default_provider.clone()
        }
    }

    pub async fn configured_status(&self) -> PlatformSecretsStatus {
        let c = self.inner.read().await;
        PlatformSecretsStatus {
            anthropic: !c.anthropic_api_key.trim().is_empty(),
            tokenmix: !c.tokenmix_api_key.trim().is_empty(),
            canopy: !c.canopy_api_key.trim().is_empty(),
            dataforseo: !c.dataforseo_login.trim().is_empty()
                && !c.dataforseo_password.trim().is_empty(),
            default_provider: c.default_provider.clone(),
        }
    }

    /// Full values for Admin UI (operator-only route).
    pub async fn admin_get(&self) -> PlatformSecretsAdminGet {
        let c = self.get().await;
        let dp = self.default_provider().await;
        PlatformSecretsAdminGet {
            anthropic: !c.anthropic_api_key.trim().is_empty(),
            tokenmix: !c.tokenmix_api_key.trim().is_empty(),
            canopy: !c.canopy_api_key.trim().is_empty(),
            dataforseo: !c.dataforseo_login.trim().is_empty()
                && !c.dataforseo_password.trim().is_empty(),
            default_provider: dp,
            anthropic_api_key: c.anthropic_api_key,
            tokenmix_api_key: c.tokenmix_api_key,
            canopy_api_key: c.canopy_api_key,
            dataforseo_login: c.dataforseo_login,
            dataforseo_password: c.dataforseo_password,
            clerk_publishable_key: c.clerk_publishable_key.clone(),
            clerk_jwt_issuer: c.clerk_jwt_issuer.clone(),
            clerk_enabled: !c.clerk_jwt_issuer.trim().is_empty(),
            bootstrap_admin_email: normalize_bootstrap_email(&c.bootstrap_admin_email),
            admin_bypass_token: c.admin_bypass_token.clone(),
        }
    }

    pub fn admin_bypass_valid(stored: &str, presented: &str) -> bool {
        use subtle::ConstantTimeEq;
        let a = stored.trim();
        let b = presented.trim();
        if a.is_empty() || b.is_empty() {
            return false;
        }
        a.as_bytes().ct_eq(b.as_bytes()).into()
    }

    pub fn clerk_config(creds: &PlatformCredentials) -> Option<(String, String)> {
        let issuer = creds.clerk_jwt_issuer.trim().trim_end_matches('/');
        if issuer.is_empty() {
            return None;
        }
        Some((
            creds.clerk_publishable_key.trim().to_string(),
            issuer.to_string(),
        ))
    }
}

#[derive(Debug, serde::Serialize)]
pub struct PlatformSecretsStatus {
    pub anthropic: bool,
    pub tokenmix: bool,
    pub canopy: bool,
    pub dataforseo: bool,
    pub default_provider: String,
}

/// GET /api/admin/platform-secrets — configured flags plus current values for editing.
#[derive(Debug, serde::Serialize)]
pub struct PlatformSecretsAdminGet {
    pub anthropic: bool,
    pub tokenmix: bool,
    pub canopy: bool,
    pub dataforseo: bool,
    pub default_provider: String,
    pub anthropic_api_key: String,
    pub tokenmix_api_key: String,
    pub canopy_api_key: String,
    pub dataforseo_login: String,
    pub dataforseo_password: String,
    pub clerk_publishable_key: String,
    pub clerk_jwt_issuer: String,
    pub clerk_enabled: bool,
    pub bootstrap_admin_email: String,
    pub admin_bypass_token: String,
}

/// Email for the first local admin (`users` row) when Clerk is off.
pub fn normalize_bootstrap_email(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        "admin@local".into()
    } else {
        t.to_string()
    }
}

/// Read a bootstrap admin email from the `users` table (since platform_secrets table is dropped).
pub async fn bootstrap_admin_email_from_env() -> String {
    normalize_bootstrap_email(&env_or_empty("bootstrap_admin_email"))
}

/// Operator user row (no Clerk) for break-glass sessions and usage attribution.
pub async fn ensure_bootstrap_user(pool: &PgPool) -> Result<Uuid, String> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM users WHERE clerk_id IS NULL OR clerk_id = '' ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    if let Some(id) = existing {
        backfill_story_owners(pool, id).await?;
        return Ok(id);
    }

    let email = normalize_bootstrap_email(&env_or_empty("bootstrap_admin_email"));
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, role, plan_label, monthly_fee_cents)
         VALUES ($1, 'subscriber', 'operator', 0)
         RETURNING id",
    )
    .bind(&email)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    backfill_story_owners(pool, id).await?;
    log::info!("Operator user created for break-glass: {email} ({id})");
    Ok(id)
}

async fn backfill_story_owners(pool: &PgPool, user_id: Uuid) -> Result<(), String> {
    sqlx::query("UPDATE stories SET owner_user_id = $1 WHERE owner_user_id IS NULL")
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ─── Helper functions ────────────────────────────────────────────────────────

fn env_or_empty(name: &str) -> String {
    std::env::var(name).unwrap_or_default().trim().to_string()
}

fn env_or_default(name: &str, default: &str) -> String {
    let val = std::env::var(name).unwrap_or_default().trim().to_string();
    if val.is_empty() {
        default.to_string()
    } else {
        val
    }
}

fn log_configured_services(creds: &PlatformCredentials) {
    let services = [
        ("TokenMix", !creds.tokenmix_api_key.is_empty()),
        ("Anthropic", !creds.anthropic_api_key.is_empty()),
        ("Canopy", !creds.canopy_api_key.is_empty()),
        (
            "DataForSEO",
            !creds.dataforseo_login.is_empty() && !creds.dataforseo_password.is_empty(),
        ),
    ];
    for (name, configured) in &services {
        if *configured {
            log::info!("Service {name}: configured");
        } else {
            log::warn!("Service {name}: NOT configured (env var empty or unset)");
        }
    }
}
