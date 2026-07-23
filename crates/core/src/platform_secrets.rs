//! Platform credentials (encrypted in DB) and runtime accessors.

use std::sync::Arc;
use tokio::sync::RwLock;

use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;
use crate::secrets::{decrypt_field, encrypt_field, encryption_key_or_dev_default};

#[derive(Clone, Debug, Default)]
pub struct PlatformCredentials {
    pub anthropic_api_key: String,
    pub tokenmix_api_key: String,
    pub canopy_api_key: String,
    pub dataforseo_login: String,
    pub dataforseo_password: String,
    pub default_provider: String,
}

#[derive(Clone)]
pub struct PlatformSecrets {
    inner: Arc<RwLock<PlatformCredentials>>,
    pool: PgPool,
    key: [u8; 32],
}

impl PlatformSecrets {
    pub async fn load(pool: PgPool, config: &Config) -> Result<Self, String> {
        let key = encryption_key_or_dev_default()?;
        let creds = load_from_db(&pool, &key).await?;
        let creds = if creds.is_empty() {
            let from_env = credentials_from_config(config);
            if !from_env.is_empty() {
                save_to_db(&pool, &key, &from_env).await?;
                from_env
            } else {
                from_env
            }
        } else {
            creds
        };
        Ok(Self {
            inner: Arc::new(RwLock::new(creds)),
            pool,
            key,
        })
    }

    pub async fn reload(&self) -> Result<(), String> {
        let creds = load_from_db(&self.pool, &self.key).await?;
        *self.inner.write().await = creds;
        Ok(())
    }

    pub async fn get(&self) -> PlatformCredentials {
        self.inner.read().await.clone()
    }

    pub async fn update(&self, patch: PlatformCredentialsPatch) -> Result<(), String> {
        let mut creds = self.inner.read().await.clone();
        apply_patch_field(&mut creds.anthropic_api_key, patch.anthropic_api_key);
        apply_patch_field(&mut creds.tokenmix_api_key, patch.tokenmix_api_key);
        apply_patch_field(&mut creds.canopy_api_key, patch.canopy_api_key);
        apply_patch_field(&mut creds.dataforseo_login, patch.dataforseo_login);
        apply_patch_field(&mut creds.dataforseo_password, patch.dataforseo_password);
        if let Some(v) = patch.default_provider {
            if !v.is_empty() {
                creds.default_provider = v;
            }
        }
        save_to_db(&self.pool, &self.key, &creds).await?;
        *self.inner.write().await = creds;
        Ok(())
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
        (c.dataforseo_login.clone(), c.dataforseo_password.clone())
    }

    pub async fn default_provider(&self) -> String {
        self.inner.read().await.default_provider.clone()
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
}

impl PlatformCredentials {
    fn is_empty(&self) -> bool {
        self.anthropic_api_key.is_empty()
            && self.tokenmix_api_key.is_empty()
            && self.canopy_api_key.is_empty()
            && self.dataforseo_login.is_empty()
            && self.dataforseo_password.is_empty()
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

#[derive(Debug, serde::Deserialize)]
pub struct PlatformCredentialsPatch {
    #[serde(default)]
    pub anthropic_api_key: Option<String>,
    #[serde(default)]
    pub tokenmix_api_key: Option<String>,
    #[serde(default)]
    pub canopy_api_key: Option<String>,
    #[serde(default)]
    pub dataforseo_login: Option<String>,
    #[serde(default)]
    pub dataforseo_password: Option<String>,
    #[serde(default)]
    pub default_provider: Option<String>,
}

fn apply_patch_field(current: &mut String, patch: Option<String>) {
    if let Some(v) = patch {
        if !v.is_empty() {
            *current = v;
        }
    }
}

fn credentials_from_config(config: &Config) -> PlatformCredentials {
    PlatformCredentials {
        anthropic_api_key: config.anthropic_api_key.clone(),
        tokenmix_api_key: config.tokenmix_api_key.clone(),
        canopy_api_key: config.canopy_api_key.clone(),
        dataforseo_login: config.dataforseo_login.clone(),
        dataforseo_password: config.dataforseo_password.clone(),
        default_provider: config.default_provider.clone(),
    }
}

async fn load_from_db(pool: &PgPool, key: &[u8; 32]) -> Result<PlatformCredentials, String> {
    let row: Option<(
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        String,
    )> = sqlx::query_as(
        "SELECT anthropic_api_key, tokenmix_api_key, canopy_api_key, dataforseo_login, dataforseo_password, default_provider FROM platform_secrets WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    let Some((a, t, c, l, p, dp)) = row else {
        return Ok(PlatformCredentials::default());
    };

    Ok(PlatformCredentials {
        anthropic_api_key: decrypt_field(&a, key)?,
        tokenmix_api_key: decrypt_field(&t, key)?,
        canopy_api_key: decrypt_field(&c, key)?,
        dataforseo_login: decrypt_field(&l, key)?,
        dataforseo_password: decrypt_field(&p, key)?,
        default_provider: if dp.is_empty() {
            "claude".into()
        } else {
            dp
        },
    })
}

async fn save_to_db(
    pool: &PgPool,
    key: &[u8; 32],
    creds: &PlatformCredentials,
) -> Result<(), String> {
    let dp = if creds.default_provider.is_empty() {
        "claude"
    } else {
        creds.default_provider.as_str()
    };
    sqlx::query(
        "UPDATE platform_secrets SET
            anthropic_api_key = $1,
            tokenmix_api_key = $2,
            canopy_api_key = $3,
            dataforseo_login = $4,
            dataforseo_password = $5,
            default_provider = $6,
            updated_at = now()
         WHERE id = 1",
    )
    .bind(encrypt_field(&creds.anthropic_api_key, key))
    .bind(encrypt_field(&creds.tokenmix_api_key, key))
    .bind(encrypt_field(&creds.canopy_api_key, key))
    .bind(encrypt_field(&creds.dataforseo_login, key))
    .bind(encrypt_field(&creds.dataforseo_password, key))
    .bind(dp)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// First admin user for pre-Clerk bootstrap.
pub async fn ensure_bootstrap_user(pool: &PgPool) -> Result<Uuid, String> {
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE role = 'admin' ORDER BY created_at LIMIT 1")
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    if let Some(id) = existing {
        backfill_story_owners(pool, id).await?;
        return Ok(id);
    }

    let email = std::env::var("BOOTSTRAP_ADMIN_EMAIL").unwrap_or_else(|_| "admin@local".into());
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, role, plan_label, monthly_fee_cents)
         VALUES ($1, 'admin', 'admin', 0)
         RETURNING id",
    )
    .bind(&email)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    backfill_story_owners(pool, id).await?;
    log::info!("Bootstrap admin user created: {email} ({id})");
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
