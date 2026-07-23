//! Clerk JWT verification and request auth.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{FromRequestParts, State};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use axum::http::{header::AUTHORIZATION, StatusCode};
use axum::response::{IntoResponse, Response};
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use loremetry_core::users;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::RwLock;

use crate::error::ok_json;
use crate::state::AppState;

#[derive(Clone, Debug)]
pub struct ClerkConfig {
    pub publishable_key: String,
    pub jwt_issuer: String,
}

impl ClerkConfig {
    pub fn from_env() -> Option<Self> {
        let jwt_issuer = std::env::var("CLERK_JWT_ISSUER")
            .ok()
            .filter(|s| !s.trim().is_empty())?;
        let publishable_key = std::env::var("CLERK_PUBLISHABLE_KEY")
            .or_else(|_| std::env::var("VITE_CLERK_PUBLISHABLE_KEY"))
            .unwrap_or_default();
        Some(Self {
            publishable_key,
            jwt_issuer: jwt_issuer.trim_end_matches('/').to_string(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub clerk_id: String,
    pub db_user_id: uuid::Uuid,
    pub email: String,
    pub role: String,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    pub fn bootstrap(state: &AppState) -> Self {
        Self {
            clerk_id: String::new(),
            db_user_id: state.ctx.user_id(),
            email: std::env::var("BOOTSTRAP_ADMIN_EMAIL").unwrap_or_else(|_| "admin@local".into()),
            role: "admin".into(),
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

#[derive(Clone)]
pub struct AuthState {
    pub clerk: Option<ClerkConfig>,
    pub jwks: Option<JwksCache>,
}

impl AuthState {
    pub fn from_env() -> Self {
        let clerk = ClerkConfig::from_env();
        let jwks = clerk.as_ref().map(|c| JwksCache::new(c.jwt_issuer.clone()));
        Self { clerk, jwks }
    }

    pub fn enabled(&self) -> bool {
        self.clerk.is_some()
    }
}

#[derive(Debug, Deserialize)]
struct ClerkJwtClaims {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
    #[serde(default)]
    public_metadata: Option<serde_json::Value>,
}

fn clerk_role_from_claims(claims: &ClerkJwtClaims) -> String {
    if claims.role.as_deref() == Some("admin") {
        return "admin".into();
    }
    for meta in [&claims.public_metadata, &claims.metadata] {
        if let Some(v) = meta {
            if v.get("role").and_then(|r| r.as_str()) == Some("admin") {
                return "admin".into();
            }
        }
    }
    "subscriber".into()
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
    auth: &AuthState,
    token: &str,
) -> Result<ClerkJwtClaims, String> {
    let jwks = auth
        .jwks
        .as_ref()
        .ok_or("Clerk JWKS not configured")?;
    let header = decode_header(token).map_err(|e| format!("JWT header: {e}"))?;
    let kid = header.kid.ok_or("JWT missing kid")?;
    let set = jwks.jwks().await?;
    let jwk = set
        .find(&kid)
        .ok_or_else(|| format!("JWKS missing key {kid}"))?;
    let dec_key = DecodingKey::from_jwk(jwk).map_err(|e| format!("JWK: {e}"))?;
    let issuer = auth
        .clerk
        .as_ref()
        .map(|c| c.jwt_issuer.clone())
        .ok_or("Clerk issuer not configured")?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[issuer.as_str()]);
    let token_data = decode::<ClerkJwtClaims>(token, &dec_key, &validation)
        .map_err(|e| format!("JWT invalid: {e}"))?;
    Ok(token_data.claims)
}

pub async fn resolve_auth_user(state: &AppState, headers: &HeaderMap) -> Result<AuthUser, String> {
    let auth = &state.auth;
    if !auth.enabled() {
        return Ok(AuthUser::bootstrap(state));
    }
    let token = bearer_from_headers(headers).ok_or("Missing Authorization: Bearer session token")?;
    let claims = verify_clerk_token(auth, &token).await?;
    let clerk_role = clerk_role_from_claims(&claims);
    let email = claims.email.unwrap_or_default();
    let (db_user_id, role) =
        users::upsert_clerk_user(&state.ctx.db.pool, &claims.sub, &email, &clerk_role).await?;
    Ok(AuthUser {
        clerk_id: claims.sub,
        db_user_id,
        email,
        role,
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

/// Admin-only authenticated request.
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
                axum::Json(json!({ "error": "Admin role required" })),
            )
                .into_response());
        }
        Ok(AdminAuthenticated(inner))
    }
}

impl FromRequestParts<AppState> for Authenticated {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        if !state.auth.enabled() {
            return Ok(Authenticated {
                state: state.clone(),
                user: AuthUser::bootstrap(state),
            });
        }
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
    let enabled = state.auth.enabled();
    let publishable_key = state
        .auth
        .clerk
        .as_ref()
        .map(|c| c.publishable_key.clone())
        .unwrap_or_default();
    ok_json(json!({
        "clerkEnabled": enabled,
        "publishableKey": publishable_key,
    }))
}

/// GET /api/me — current user.
pub async fn auth_me(auth: Authenticated) -> impl IntoResponse {
    ok_json(json!({
        "id": auth.user.db_user_id,
        "email": auth.user.email,
        "role": auth.user.role,
        "isAdmin": auth.user.is_admin(),
    }))
}
