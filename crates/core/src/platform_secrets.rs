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
    pub clerk_publishable_key: String,
    pub clerk_jwt_issuer: String,
    pub bootstrap_admin_email: String,
    pub admin_bypass_token: String,
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
        apply_patch_field(&mut creds.clerk_publishable_key, patch.clerk_publishable_key);
        apply_patch_field(&mut creds.clerk_jwt_issuer, patch.clerk_jwt_issuer);
        apply_patch_field(&mut creds.bootstrap_admin_email, patch.bootstrap_admin_email);
        apply_patch_field(&mut creds.admin_bypass_token, patch.admin_bypass_token);
        creds.bootstrap_admin_email = normalize_bootstrap_email(&creds.bootstrap_admin_email);
        if let Some(v) = patch.default_provider {
            if !v.is_empty() {
                creds.default_provider = v;
            }
        }
        save_to_db(
            &self.pool,
        self.key
            .as_ref()
            .ok_or(
                "Cannot save credentials: set a valid SECRETS_ENCRYPTION_KEY in the server environment (paste the output of `openssl rand -base64 32`, not the command text), redeploy, then save again.",
            )?,
            &creds,
        )
        .await?;
        sync_local_admin_email(&self.pool, &creds.bootstrap_admin_email).await?;
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
    #[serde(default)]
    pub clerk_publishable_key: Option<String>,
    #[serde(default)]
    pub clerk_jwt_issuer: Option<String>,
    #[serde(default)]
    pub bootstrap_admin_email: Option<String>,
    #[serde(default)]
    pub admin_bypass_token: Option<String>,
}

fn apply_patch_field(current: &mut String, patch: Option<String>) {
    if let Some(v) = patch {
        if !v.is_empty() {
            *current = v;
        }
    }
}

/// Email for the first local admin (`users` row) when Clerk is off; stored in `platform_secrets`.
pub fn normalize_bootstrap_email(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        "admin@local".into()
    } else {
        t.to_string()
    }
}

pub async fn bootstrap_admin_email_from_db(pool: &PgPool) -> Result<String, String> {
    let row: Option<String> =
        sqlx::query_scalar("SELECT bootstrap_admin_email FROM platform_secrets WHERE id = 1")
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    Ok(normalize_bootstrap_email(row.as_deref().unwrap_or("")))
}

async fn sync_local_admin_email(pool: &PgPool, email: &str) -> Result<(), String> {
    sqlx::query(
        "UPDATE users SET email = $1
         WHERE clerk_id IS NULL OR clerk_id = ''",
    )
    .bind(email)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
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
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT anthropic_api_key, tokenmix_api_key, canopy_api_key, dataforseo_login, dataforseo_password, default_provider, clerk_publishable_key, clerk_jwt_issuer, bootstrap_admin_email, admin_bypass_token FROM platform_secrets WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    let Some((a, t, c, l, p, dp, clerk_pk, clerk_iss, bootstrap_email, bypass)) = row else {
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
        clerk_publishable_key: clerk_pk,
        clerk_jwt_issuer: clerk_iss,
        bootstrap_admin_email: normalize_bootstrap_email(&bootstrap_email),
        admin_bypass_token: bypass,
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
    let bootstrap_email = normalize_bootstrap_email(&creds.bootstrap_admin_email);
    sqlx::query(
        "INSERT INTO platform_secrets (
            id, anthropic_api_key, tokenmix_api_key, canopy_api_key,
            dataforseo_login, dataforseo_password, default_provider,
            clerk_publishable_key, clerk_jwt_issuer, bootstrap_admin_email, admin_bypass_token
         ) VALUES (1, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (id) DO UPDATE SET
            anthropic_api_key = EXCLUDED.anthropic_api_key,
            tokenmix_api_key = EXCLUDED.tokenmix_api_key,
            canopy_api_key = EXCLUDED.canopy_api_key,
            dataforseo_login = EXCLUDED.dataforseo_login,
            dataforseo_password = EXCLUDED.dataforseo_password,
            default_provider = EXCLUDED.default_provider,
            clerk_publishable_key = EXCLUDED.clerk_publishable_key,
            clerk_jwt_issuer = EXCLUDED.clerk_jwt_issuer,
            bootstrap_admin_email = EXCLUDED.bootstrap_admin_email,
            admin_bypass_token = EXCLUDED.admin_bypass_token,
            updated_at = now()",
    )
    .bind(encrypt_field(&creds.anthropic_api_key, key))
    .bind(encrypt_field(&creds.tokenmix_api_key, key))
    .bind(encrypt_field(&creds.canopy_api_key, key))
    .bind(encrypt_field(&creds.dataforseo_login, key))
    .bind(encrypt_field(&creds.dataforseo_password, key))
    .bind(dp)
    .bind(creds.clerk_publishable_key.trim())
    .bind(creds.clerk_jwt_issuer.trim().trim_end_matches('/'))
    .bind(&bootstrap_email)
    .bind(creds.admin_bypass_token.trim())
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
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

    let email = bootstrap_admin_email_from_db(pool).await?;
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

fn generate_operator_bypass_token() -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    STANDARD.encode(bytes)
}

/// If no operator bypass is configured, generate one and persist (plaintext). Returns the new token.
pub async fn ensure_operator_bypass_token(pool: &PgPool) -> Result<Option<String>, String> {
    let current: Option<String> =
        sqlx::query_scalar("SELECT admin_bypass_token FROM platform_secrets WHERE id = 1")
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    if current.as_deref().unwrap_or("").trim().len() > 0 {
        return Ok(None);
    }

    let token = generate_operator_bypass_token();
    let result = sqlx::query(
        "UPDATE platform_secrets SET admin_bypass_token = $1, updated_at = now() WHERE id = 1",
    )
    .bind(&token)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    if result.rows_affected() == 0 {
        sqlx::query(
            "INSERT INTO platform_secrets (id, admin_bypass_token) VALUES (1, $1)
             ON CONFLICT (id) DO UPDATE SET
                admin_bypass_token = EXCLUDED.admin_bypass_token,
                updated_at = now()",
        )
        .bind(&token)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(Some(token))
}
