//! Maintenance mode middleware.
//!
//! Returns 503 to non-admin requests when the platform's primary AI service
//! (TokenMix) is not yet configured.

use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::body::Body;
use serde_json::json;

use crate::auth::ADMIN_BYPASS_HEADER;
use crate::state::AppState;
use loremetry_core::platform_secrets::PlatformSecrets;

/// Axum middleware that gates requests behind maintenance mode.
///
/// Pass-through when:
/// - TokenMix is configured (platform is operational), OR
/// - The request carries a valid admin bypass token.
///
/// Otherwise responds with 503 Service Unavailable.
pub async fn maintenance_guard(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    // Fast path: platform is configured — let everything through.
    let status = state.secrets.configured_status().await;
    if status.tokenmix {
        return next.run(request).await;
    }

    // Platform unconfigured — check for admin bypass.
    let is_admin = {
        let presented = request
            .headers()
            .get(ADMIN_BYPASS_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty());

        if let Some(token) = presented {
            let creds = state.secrets.get().await;
            PlatformSecrets::admin_bypass_valid(&creds.admin_bypass_token, token)
        } else {
            false
        }
    };

    if is_admin {
        return next.run(request).await;
    }

    // Non-admin on unconfigured platform → 503 maintenance response.
    (
        StatusCode::SERVICE_UNAVAILABLE,
        axum::Json(json!({
            "error": "maintenance",
            "message": "The platform is being configured. Please try again later."
        })),
    )
        .into_response()
}
