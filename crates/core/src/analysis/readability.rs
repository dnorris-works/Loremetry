// analysis/readability.rs — Client-generated readability report persistence.
//
// Metrics are computed in the browser; the server only stores the JSON report.

use super::{err, GenreResult};
use crate::app_ctx::AppCtx;
use crate::db;

#[derive(serde::Deserialize)]
pub struct SaveReadabilityClientRequest {
    #[serde(alias = "folder")]
    pub story_id: String,
    pub content: String,
}

/// Persist a readability report generated in the browser.
pub async fn save_readability_client_report(
    app: AppCtx,
    request: SaveReadabilityClientRequest,
) -> GenreResult {
    if !crate::stories::story_exists(&app.db, &request.story_id).await {
        return err("Story not found.");
    }

    let json: serde_json::Value = match serde_json::from_str(&request.content) {
        Ok(v) => v,
        Err(e) => return err(&format!("Invalid readability JSON: {e}")),
    };

    if json.get("schema").and_then(|v| v.as_str()) != Some("readability_v1") {
        return err("Expected schema readability_v1.");
    }

    let database = app.db.as_ref();
    let run_ts = chrono::Utc::now().to_rfc3339();
    if let Err(e) = db::save_document_at(
        &database.pool,
        &request.story_id,
        "readability_analysis",
        &request.content,
        &run_ts,
    )
    .await
    {
        return err(&format!("Could not save report document: {e}"));
    }

    GenreResult {
        success: true,
        report: request.content,
        error: String::new(),
        run_ts,
    }
}
