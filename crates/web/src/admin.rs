//! Admin routes — settings, catalog import, SQL console (no auth).

use std::time::Instant;

use axum::extract::{Multipart, Query};
use axum::response::IntoResponse;
use axum::Json;
use loremetry_core::platform_secrets::PlatformCredentialsPatch;
use loremetry_core::usage::{usage_events, usage_summary};
use loremetry_core::{canopy, dataforseo, winningcat};
use serde::Deserialize;
use serde_json::json;
use sqlx::{Column, Row, ValueRef};

use crate::auth::AdminAuthenticated;
use crate::error::{json_error, ok_json};
use crate::state::AppState;

/// GET /api/admin/status
pub async fn admin_status() -> impl IntoResponse {
    ok_json(json!({ "configured": true }))
}

#[derive(Deserialize)]
pub struct WinningCatJsonBody {
    csv_text: String,
}

/// POST /api/admin/winningcat/import — JSON `{ csv_text }`.
pub async fn winningcat_import_json(
    auth: AdminAuthenticated,
    Json(body): Json<WinningCatJsonBody>,
) -> impl IntoResponse {
    ok_json(
        serde_json::to_value(winningcat::import_winningcat_csv(auth.ctx(), body.csv_text).await)
            .unwrap_or(json!(null)),
    )
}

/// POST /api/admin/winningcat/upload — multipart CSV file.
pub async fn winningcat_import_upload(
    auth: AdminAuthenticated,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let csv_text = match read_csv_from_multipart(&mut multipart).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    ok_json(
        serde_json::to_value(winningcat::import_winningcat_csv(auth.ctx(), csv_text).await)
            .unwrap_or(json!(null)),
    )
}

async fn read_csv_from_multipart(multipart: &mut Multipart) -> Result<String, axum::response::Response> {
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
    auth: AdminAuthenticated,
    Json(body): Json<StaleBody>,
) -> impl IntoResponse {
    ok_json(
        serde_json::to_value(winningcat::remove_stale_kdp_categories(auth.ctx(), body.since).await)
            .unwrap_or(json!(null)),
    )
}

/// GET /api/admin/tables — all application tables in `lore` and `public` (including lookup/config).
pub async fn admin_tables(auth: AdminAuthenticated) -> impl IntoResponse {
    let pool = &auth.ctx().db.pool;
    let result = sqlx::query_as::<_, (String, String)>(
        "SELECT schemaname::text, tablename::text
         FROM pg_catalog.pg_tables
         WHERE schemaname IN ('lore', 'public')
         ORDER BY schemaname, tablename",
    )
    .fetch_all(pool)
    .await;

    match result {
        Ok(rows) => {
            let tables: Vec<_> = rows
                .into_iter()
                .map(|(schema, name)| {
                    json!({
                        "schema": schema,
                        "name": name,
                        "qualified": format!("{schema}.{name}"),
                    })
                })
                .collect();
            ok_json(json!({ "success": true, "tables": tables }))
        }
        Err(e) => ok_json(json!({ "success": false, "error": e.to_string(), "tables": [] })),
    }
}

#[derive(Deserialize)]
pub struct SqlBody {
    sql: String,
}

/// POST /api/admin/sql — run arbitrary SQL (operator tool).
pub async fn admin_sql(auth: AdminAuthenticated, Json(body): Json<SqlBody>) -> impl IntoResponse {
    let sql = body.sql.trim().trim_end_matches(';');
    if sql.is_empty() {
        return ok_json(json!({ "success": false, "error": "Empty query" }));
    }

    let pool = &auth.ctx().db.pool;
    let started = Instant::now();
    let lower = sql.to_lowercase();
    let returns_rows = lower.starts_with("select")
        || lower.starts_with("with")
        || lower.starts_with("show")
        || lower.starts_with("explain")
        || lower.starts_with("table ")
        || lower.starts_with("\\d");

    if returns_rows {
        match sqlx::query(sql).fetch_all(pool).await {
            Ok(rows) => {
                let columns: Vec<String> = rows
                    .first()
                    .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
                    .unwrap_or_default();
                let out_rows: Vec<Vec<serde_json::Value>> =
                    rows.iter().map(row_to_json).collect();
                ok_json(json!({
                    "success": true,
                    "columns": columns,
                    "rows": out_rows,
                    "rows_affected": rows.len(),
                    "duration_ms": started.elapsed().as_millis(),
                }))
            }
            Err(e) => ok_json(json!({ "success": false, "error": e.to_string() })),
        }
    } else {
        match sqlx::query(sql).execute(pool).await {
            Ok(result) => ok_json(json!({
                "success": true,
                "columns": [],
                "rows": [],
                "rows_affected": result.rows_affected(),
                "duration_ms": started.elapsed().as_millis(),
            })),
            Err(e) => ok_json(json!({ "success": false, "error": e.to_string() })),
        }
    }
}

/// GET /api/admin/platform-secrets
pub async fn get_platform_secrets(auth: AdminAuthenticated) -> impl IntoResponse {
    let view = auth.state.secrets.admin_get().await;
    ok_json(serde_json::to_value(view).unwrap_or(json!(null)))
}

/// PUT /api/admin/platform-secrets
pub async fn put_platform_secrets(
    auth: AdminAuthenticated,
    Json(patch): Json<PlatformCredentialsPatch>,
) -> impl IntoResponse {
    match auth.state.secrets.update(patch).await {
        Ok(()) => {
            auth.state.jwt.reset_cache().await;
            let status = auth.state.secrets.configured_status().await;
            ok_json(json!({ "success": true, "configured": status }))
        }
        Err(e) => ok_json(json!({ "success": false, "error": e })),
    }
}

/// POST /api/admin/platform-secrets/test-canopy
#[derive(Deserialize, Default)]
pub struct TestCanopyBody {
    #[serde(default)]
    pub canopy_api_key: Option<String>,
}

pub async fn test_platform_canopy(
    auth: AdminAuthenticated,
    Json(body): Json<TestCanopyBody>,
) -> impl IntoResponse {
    let key = match body
        .canopy_api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(k) => k.to_string(),
        None => auth.state.secrets.canopy_key().await,
    };
    ok_json(serde_json::to_value(canopy::test_canopy_connection(key).await).unwrap_or(json!(null)))
}

/// POST /api/admin/platform-secrets/test-dataforseo
#[derive(Deserialize, Default)]
pub struct TestDataforseoBody {
    #[serde(default)]
    pub dataforseo_login: Option<String>,
    #[serde(default)]
    pub dataforseo_password: Option<String>,
}

pub async fn test_platform_dataforseo(
    auth: AdminAuthenticated,
    Json(body): Json<TestDataforseoBody>,
) -> impl IntoResponse {
    let (login, password) = match (
        body.dataforseo_login
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        body.dataforseo_password
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    ) {
        (Some(l), Some(p)) => (l.to_string(), p.to_string()),
        _ => auth.state.secrets.dataforseo().await,
    };
    ok_json(
        serde_json::to_value(dataforseo::test_dataforseo_connection(login, password).await)
            .unwrap_or(json!(null)),
    )
}

#[derive(Deserialize)]
pub struct UsageQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

/// GET /api/admin/usage/summary
pub async fn admin_usage_summary(
    auth: AdminAuthenticated,
    Query(q): Query<UsageQuery>,
) -> impl IntoResponse {
    let now = chrono::Utc::now();
    let from = q
        .from
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or_else(|| now - chrono::Duration::days(30));
    let to = q
        .to
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or(now);

    match usage_summary(&auth.ctx().db.pool, from, to).await {
        Ok(rows) => ok_json(json!({ "success": true, "from": from, "to": to, "users": rows })),
        Err(e) => ok_json(json!({ "success": false, "error": e, "users": [] })),
    }
}

#[derive(Deserialize)]
pub struct UsageEventsQuery {
    pub user_id: uuid::Uuid,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    100
}

/// GET /api/admin/usage/events
pub async fn admin_usage_events(
    auth: AdminAuthenticated,
    Query(q): Query<UsageEventsQuery>,
) -> impl IntoResponse {
    match usage_events(&auth.ctx().db.pool, q.user_id, q.limit.min(500)).await {
        Ok(events) => ok_json(json!({ "success": true, "events": events })),
        Err(e) => ok_json(json!({ "success": false, "error": e, "events": [] })),
    }
}

fn row_to_json(row: &sqlx::postgres::PgRow) -> Vec<serde_json::Value> {
    (0..row.len())
        .map(|i| cell_to_json(row, i))
        .collect()
}

fn cell_to_json(row: &sqlx::postgres::PgRow, i: usize) -> serde_json::Value {
    use sqlx::TypeInfo;
    if row.try_get_raw(i).ok().map(|r| r.is_null()).unwrap_or(true) {
        return serde_json::Value::Null;
    }
    let type_name = row.columns()[i].type_info().name();
    match type_name {
        "BOOL" => row
            .try_get::<bool, _>(i)
            .map(|v| json!(v))
            .unwrap_or(serde_json::Value::Null),
        "INT2" | "INT4" | "INT8" => row
            .try_get::<i64, _>(i)
            .map(|v| json!(v))
            .unwrap_or(serde_json::Value::Null),
        "FLOAT4" | "FLOAT8" | "NUMERIC" => row
            .try_get::<f64, _>(i)
            .map(|v| json!(v))
            .unwrap_or(serde_json::Value::Null),
        "JSON" | "JSONB" => row
            .try_get::<serde_json::Value, _>(i)
            .unwrap_or(serde_json::Value::Null),
        _ => row
            .try_get::<String, _>(i)
            .map(|v| json!(v))
            .unwrap_or_else(|_| {
                row.try_get::<Vec<u8>, _>(i)
                    .map(|b| json!(format!("<bytea {} bytes>", b.len())))
                    .unwrap_or(serde_json::Value::Null)
            }),
    }
}
