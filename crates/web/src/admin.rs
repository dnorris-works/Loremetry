//! Admin routes — settings, catalog import, SQL console (no auth).

use std::time::Instant;

use axum::extract::{Multipart, State};
use axum::response::IntoResponse;
use axum::Json;
use loremetry_core::winningcat;
use serde::Deserialize;
use serde_json::json;
use sqlx::{Column, Row, ValueRef};

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
    State(state): State<AppState>,
    Json(body): Json<WinningCatJsonBody>,
) -> impl IntoResponse {
    ok_json(
        serde_json::to_value(winningcat::import_winningcat_csv(state.ctx, body.csv_text).await)
            .unwrap_or(json!(null)),
    )
}

/// POST /api/admin/winningcat/upload — multipart CSV file.
pub async fn winningcat_import_upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let csv_text = match read_csv_from_multipart(&mut multipart).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    ok_json(
        serde_json::to_value(winningcat::import_winningcat_csv(state.ctx, csv_text).await)
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
    State(state): State<AppState>,
    Json(body): Json<StaleBody>,
) -> impl IntoResponse {
    ok_json(
        serde_json::to_value(winningcat::remove_stale_kdp_categories(state.ctx, body.since).await)
            .unwrap_or(json!(null)),
    )
}

/// GET /api/admin/tables — tables in `lore` and `public` schemas.
pub async fn admin_tables(State(state): State<AppState>) -> impl IntoResponse {
    let pool = &state.ctx.db.0;
    let result = sqlx::query_as::<_, (String, String)>(
        "SELECT table_schema, table_name
         FROM information_schema.tables
         WHERE table_schema IN ('lore', 'public')
           AND table_type = 'BASE TABLE'
           AND table_name NOT LIKE '\\_%' ESCAPE '\\'
         ORDER BY table_schema, table_name",
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
pub async fn admin_sql(State(state): State<AppState>, Json(body): Json<SqlBody>) -> impl IntoResponse {
    let sql = body.sql.trim().trim_end_matches(';');
    if sql.is_empty() {
        return ok_json(json!({ "success": false, "error": "Empty query" }));
    }

    let pool = &state.ctx.db.0;
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
