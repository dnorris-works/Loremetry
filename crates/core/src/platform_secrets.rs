//! Platform credentials (encrypted in DB) and runtime accessors.

use std::sync::Arc;
use tokio::sync::RwLock;

use sqlx::PgPool;
use uuid::Uuid;

use crate::secrets::{decrypt_field, encrypt_field, resolve_encryption_key};

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
    key: Option<[u8; 32]>,
}

impl PlatformSecrets {
    pub async fn load(pool: PgPool) -> Result<Self, String> {
        let key = resolve_encryption_key()?;
        let creds = load_from_db(&pool, key.as_ref()).await?;
        Ok(Self {
            inner: Arc::new(RwLock::new(creds)),
            pool,
            key,
        })
    }

    pub async fn reload(&self) -> Result<(), String> {
        let mut creds = load_from_db(&self.pool, self.key.as_ref()).await?;
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
        save_to_db(
            &self.pool,
            self.key
                .as_ref()
                .ok_or("SECRETS_ENCRYPTION_KEY must be set to update platform credentials")?,
            &creds,
        )
        .await?;
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
        (
            c.dataforseo_login.trim().to_string(),
            c.dataforseo_password.trim().to_string(),
        )
    }

    pub async fn default_provider(&self) -> String {
        let c = self.inner.read().await;
        if c.default_provider.trim().is_empty() {
            "claude".into()
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
        }
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

async fn load_from_db(
    pool: &PgPool,
    key: Option<&[u8; 32]>,
) -> Result<PlatformCredentials, String> {
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
        anthropic_api_key: decrypt_optional(&a, key)?,
        tokenmix_api_key: decrypt_optional(&t, key)?,
        canopy_api_key: decrypt_optional(&c, key)?,
        dataforseo_login: decrypt_optional(&l, key)?,
        dataforseo_password: decrypt_optional(&p, key)?,
        default_provider: if dp.is_empty() {
            "claude".into()
        } else {
            dp
        },
    })
}

fn decrypt_optional(blob: &[u8], key: Option<&[u8; 32]>) -> Result<String, String> {
    if blob.is_empty() {
        return Ok(String::new());
    }
    let Some(k) = key else {
        return Err(
            "SECRETS_ENCRYPTION_KEY is not set but encrypted platform credentials exist in the database (set the key or reset lore.platform_secrets)".into(),
        );
    };
    decrypt_field(blob, k)
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
