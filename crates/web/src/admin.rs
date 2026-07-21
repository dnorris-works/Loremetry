//! Admin-only routes (WinningCat catalog import, etc.) — require `ADMIN_TOKEN`.

use axum::extract::{Multipart, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use loremetry_core::winningcat;
use serde::Deserialize;
use serde_json::json;

use crate::error::{json_error, json_error_status, ok_json};
use crate::state::AppState;

pub fn authorize_admin(headers: &HeaderMap, state: &AppState) -> Result<(), Response> {
    if state.config.admin_token.is_empty() {
        return Err(json_error_status(
            StatusCode::SERVICE_UNAVAILABLE,
            "Admin API not configured. Set ADMIN_TOKEN on the server.",
        ));
    }
    let provided = bearer_token(headers).or_else(|| header_token(headers));
    if provided.as_deref() == Some(state.config.admin_token.as_str()) {
        Ok(())
    } else {
        Err(json_error_status(
            StatusCode::UNAUTHORIZED,
            "Invalid or missing admin token",
        ))
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ").map(|s| s.to_string())
}

fn header_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-admin-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// GET /api/admin/status — whether admin API is enabled (no auth).
pub async fn admin_status(State(state): State<AppState>) -> impl IntoResponse {
    ok_json(json!({
        "configured": !state.config.admin_token.is_empty(),
    }))
}

#[derive(Deserialize)]
pub struct WinningCatJsonBody {
    csv_text: String,
}

/// POST /api/admin/winningcat/import — JSON `{ csv_text }`.
pub async fn winningcat_import_json(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<WinningCatJsonBody>,
) -> impl IntoResponse {
    if let Err(r) = authorize_admin(&headers, &state) {
        return r;
    }
    ok_json(
        serde_json::to_value(winningcat::import_winningcat_csv(state.ctx, body.csv_text).await)
            .unwrap_or(json!(null)),
    )
}

/// POST /api/admin/winningcat/upload — multipart CSV file.
pub async fn winningcat_import_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if let Err(r) = authorize_admin(&headers, &state) {
        return r;
    }
    let csv_text = match read_csv_from_multipart(&mut multipart).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    ok_json(
        serde_json::to_value(winningcat::import_winningcat_csv(state.ctx, csv_text).await)
            .unwrap_or(json!(null)),
    )
}

async fn read_csv_from_multipart(multipart: &mut Multipart) -> Result<String, Response> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| json_error(format!("Multipart error: {e}")))?
    {
        let bytes = field
            .bytes()
            .await
            .map_err(|e| json_error(format!("Read error: {e}")))?;
        return Ok(String::from_utf8_lossy(&bytes).to_string());
    }
    Err(json_error("No file uploaded"))
}

#[derive(Deserialize)]
pub struct StaleBody {
    since: String,
}

/// POST /api/admin/winningcat/remove-stale
pub async fn winningcat_remove_stale(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<StaleBody>,
) -> impl IntoResponse {
    if let Err(r) = authorize_admin(&headers, &state) {
        return r;
    }
    ok_json(
        serde_json::to_value(winningcat::remove_stale_kdp_categories(state.ctx, body.since).await)
            .unwrap_or(json!(null)),
    )
}
