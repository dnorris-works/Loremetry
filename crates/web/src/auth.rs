//! Clerk JWT verification and request auth.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{FromRequestParts, State};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use axum::http::{header::AUTHORIZATION, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use loremetry_core::platform_secrets::PlatformSecrets;
use loremetry_core::users;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::RwLock;

use crate::error::ok_json;
use crate::state::AppState;

pub const ADMIN_BYPASS_HEADER: &str = "x-loremetry-admin-bypass";

#[derive(Clone, Debug)]
pub struct ClerkConfig {
    pub publishable_key: String,
    pub jwt_issuer: String,
}

impl ClerkConfig {
    pub fn from_credentials(
        creds: &loremetry_core::platform_secrets::PlatformCredentials,
    ) -> Option<Self> {
        let (publishable_key, jwt_issuer) = PlatformSecrets::clerk_config(creds)?;
        Some(Self {
            publishable_key,
            jwt_issuer,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub clerk_id: String,
    pub db_user_id: uuid::Uuid,
    pub email: String,
    pub role: String,
    pub theme_preference: String,
    pub plan_label: String,
    /// Operator break-glass (admin UI + platform secrets); not stored on Clerk users.
    pub break_glass: bool,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.break_glass
    }

    pub async fn break_glass_user(state: &AppState) -> Self {
        let id = state.ctx.user_id();
        let email = users::email_by_id(&state.ctx.db.pool, id)
            .await
            .unwrap_or_else(|| {
                loremetry_core::platform_secrets::normalize_bootstrap_email("")
            });
        Self {
            clerk_id: String::new(),
            db_user_id: id,
            email,
            role: "subscriber".into(),
            theme_preference: users::theme_preference_by_id(&state.ctx.db.pool, id)
                .await
                .unwrap_or_else(|| "light".to_string()),
            plan_label: users::plan_label_by_id(&state.ctx.db.pool, id)
                .await
                .unwrap_or_default(),
            break_glass: true,
        }
    }
}

#[derive(Clone)]
pub struct JwksCache {
    issuer: String,
    inner: Arc<RwLock<CachedJwks>>,
}

struct CachedJwks {
    set: Option<JwkSet>,
    fetched_at: Option<Instant>,
}

impl JwksCache {
    pub fn new(issuer: String) -> Self {
        Self {
            issuer,
            inner: Arc::new(RwLock::new(CachedJwks {
                set: None,
                fetched_at: None,
            })),
        }
    }

    async fn jwks(&self) -> Result<JwkSet, String> {
        let needs_fetch = {
            let guard = self.inner.read().await;
            guard.fetched_at.is_none()
                || guard.fetched_at.is_some_and(|t| t.elapsed() > Duration::from_secs(3600))
        };
        if needs_fetch {
            let url = format!("{}/.well-known/jwks.json", self.issuer);
            let resp = reqwest::get(&url)
                .await
                .map_err(|e| format!("JWKS fetch failed: {e}"))?;
            let set: JwkSet = resp
                .json()
                .await
                .map_err(|e| format!("JWKS parse failed: {e}"))?;
            let mut guard = self.inner.write().await;
            guard.set = Some(set.clone());
            guard.fetched_at = Some(Instant::now());
            return Ok(set);
        }
        let guard = self.inner.read().await;
        guard
            .set
            .clone()
            .ok_or_else(|| "JWKS not loaded".into())
    }
}

#[derive(Clone, Default)]
pub struct JwtVerifier {
    cache: Arc<RwLock<Option<(String, JwksCache)>>>,
}

impl JwtVerifier {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn reset_cache(&self) {
        *self.cache.write().await = None;
    }

    async fn jwks_for_issuer(&self, issuer: &str) -> Result<JwksCache, String> {
        let mut guard = self.cache.write().await;
        if let Some((iss, cache)) = guard.as_ref() {
            if iss == issuer {
                return Ok(cache.clone());
            }
        }
        let cache = JwksCache::new(issuer.to_string());
        *guard = Some((issuer.to_string(), cache.clone()));
        Ok(cache)
    }
}

async fn clerk_config_from_state(state: &AppState) -> Option<ClerkConfig> {
    let creds = state.secrets.get().await;
    ClerkConfig::from_credentials(&creds)
}

pub async fn clerk_auth_enabled(state: &AppState) -> bool {
    clerk_config_from_state(state).await.is_some()
}

fn bypass_token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(ADMIN_BYPASS_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub async fn try_break_glass(state: &AppState, headers: &HeaderMap) -> Option<AuthUser> {
    let presented = bypass_token_from_headers(headers)?;
    let creds = state.secrets.get().await;
    if !PlatformSecrets::admin_bypass_valid(&creds.admin_bypass_token, &presented) {
        return None;
    }
    tracing::info!("Operator break-glass session");
    Some(AuthUser::break_glass_user(state).await)
}

#[derive(Debug, Deserialize)]
struct ClerkJwtClaims {
    sub: String,
    #[serde(default)]
    email: Option<String>,
}

fn bearer_from_headers(headers: &HeaderMap) -> Option<String> {
    let header = headers.get(AUTHORIZATION)?.to_str().ok()?;
    header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

async fn verify_clerk_token(
    state: &AppState,
    clerk: &ClerkConfig,
    token: &str,
) -> Result<ClerkJwtClaims, String> {
    let jwks = state.jwt.jwks_for_issuer(&clerk.jwt_issuer).await?;
    let header = decode_header(token).map_err(|e| format!("JWT header: {e}"))?;
    let kid = header.kid.ok_or("JWT missing kid")?;
    let set = jwks.jwks().await?;
    let jwk = set
        .find(&kid)
        .ok_or_else(|| format!("JWKS missing key {kid}"))?;
    let dec_key = DecodingKey::from_jwk(jwk).map_err(|e| format!("JWK: {e}"))?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[clerk.jwt_issuer.as_str()]);
    let token_data = decode::<ClerkJwtClaims>(token, &dec_key, &validation)
        .map_err(|e| format!("JWT invalid: {e}"))?;
    Ok(token_data.claims)
}

pub async fn resolve_auth_user(state: &AppState, headers: &HeaderMap) -> Result<AuthUser, String> {
    if let Some(user) = try_break_glass(state, headers).await {
        return Ok(user);
    }
    let clerk = clerk_config_from_state(state)
        .await
        .ok_or("Clerk is not configured. Sign in is unavailable until Clerk is set in platform credentials.")?;
    let token = bearer_from_headers(headers).ok_or("Missing Authorization: Bearer session token")?;
    let claims = verify_clerk_token(state, &clerk, &token).await?;
    let email = claims.email.unwrap_or_default();
    let (db_user_id, role) =
        users::upsert_clerk_user(&state.ctx.db.pool, &claims.sub, &email).await?;
    let theme_preference = users::theme_preference_by_id(&state.ctx.db.pool, db_user_id)
        .await
        .unwrap_or_else(|| "light".to_string());
    let plan_label = users::plan_label_by_id(&state.ctx.db.pool, db_user_id)
        .await
        .unwrap_or_default();
    Ok(AuthUser {
        clerk_id: claims.sub,
        db_user_id,
        email,
        role,
        theme_preference,
        plan_label,
        break_glass: false,
    })
}

/// Authenticated request: app state + resolved user.
#[derive(Clone)]
pub struct Authenticated {
    pub state: AppState,
    pub user: AuthUser,
}

impl Authenticated {
    pub fn ctx(&self) -> loremetry_core::AppCtx {
        self.state.ctx.with_user_id(self.user.db_user_id)
    }
}

/// Admin-only authenticated request (operator break-glass only).
#[derive(Clone)]
pub struct AdminAuthenticated(pub Authenticated);

impl std::ops::Deref for AdminAuthenticated {
    type Target = Authenticated;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequestParts<AppState> for AdminAuthenticated {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let inner = Authenticated::from_request_parts(parts, state).await?;
        if !inner.user.is_admin() {
            return Err((
                StatusCode::FORBIDDEN,
                axum::Json(json!({ "error": "Operator access required" })),
            )
                .into_response());
        }
        Ok(AdminAuthenticated(inner))
    }
}

impl FromRequestParts<AppState> for Authenticated {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        match resolve_auth_user(state, &parts.headers).await {
            Ok(user) => Ok(Authenticated {
                state: state.clone(),
                user,
            }),
            Err(e) => Err((
                StatusCode::UNAUTHORIZED,
                axum::Json(json!({ "error": e })),
            )
                .into_response()),
        }
    }
}

/// GET /api/auth/config — public Clerk settings for the SPA.
pub async fn auth_config(State(state): State<AppState>) -> impl IntoResponse {
    let creds = state.secrets.get().await;
    let clerk = ClerkConfig::from_credentials(&creds);
    ok_json(json!({
        "clerkEnabled": clerk.is_some(),
        "publishableKey": clerk.map(|c| c.publishable_key).unwrap_or_default(),
    }))
}

/// GET /api/auth/session — silent session probe (always 200; no 401 for missing/invalid auth).
pub async fn auth_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    match resolve_auth_user(&state, &headers).await {
        Ok(user) => ok_json(json!({
            "authenticated": true,
            "id": user.db_user_id,
            "email": user.email,
            "role": user.role,
            "theme": user.theme_preference,
            "isAdmin": user.is_admin(),
            "breakGlass": user.break_glass,
        })),
        Err(e) => ok_json(json!({ "authenticated": false, "reason": e })),
    }
}

/// GET /api/me — current user.
pub async fn auth_me(auth: Authenticated) -> impl IntoResponse {
    ok_json(json!({
        "id": auth.user.db_user_id,
        "email": auth.user.email,
        "role": auth.user.role,
        "theme": auth.user.theme_preference,
        "isAdmin": auth.user.is_admin(),
        "breakGlass": auth.user.break_glass,
    }))
}

#[derive(Deserialize)]
pub struct PreferencesBody {
    pub theme: String,
}

/// PATCH /api/me/preferences — persist UI preferences (theme).
pub async fn update_preferences(
    auth: Authenticated,
    Json(body): Json<PreferencesBody>,
) -> impl IntoResponse {
    if let Err(e) = users::set_theme_preference(
        &auth.state.ctx.db.pool,
        auth.user.db_user_id,
        body.theme.trim(),
    )
    .await
    {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({ "error": e })),
        )
            .into_response();
    }
    ok_json(json!({ "success": true, "theme": body.theme.trim() }))
}

pub fn invoke_requires_operator(cmd: &str) -> bool {
    matches!(
        cmd,
        "get_platform_credentials" | "update_platform_credentials"
    )
}
