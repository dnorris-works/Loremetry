use axum::extract::{Multipart, Path, Query, State};
use axum::response::IntoResponse;
use loremetry_core::documents::{self, UpsertDocumentRequest};
use serde::Deserialize;
use serde_json::json;

use crate::error::{json_error, ok_json};
use crate::state::AppState;

#[derive(Deserialize, Default)]
pub struct UploadQuery {
    /// chapter | bible | character | location
    #[serde(default)]
    pub kind: String,
    /// When true, delete existing documents of this kind before uploading (bible only).
    #[serde(default)]
    pub replace: bool,
}

fn normalize_kind(kind: &str) -> &'static str {
    let k = kind.trim().to_lowercase();
    match k.as_str() {
        "bible" => "bible",
        "character" | "characters" => "character",
        "location" | "locations" => "location",
        _ => "chapter",
    }
}

fn path_hint_for_kind(kind: &str, filename: &str) -> String {
    let name = if filename.is_empty() { "untitled.md" } else { filename };
    match kind {
        "bible" => format!("Bible/{name}"),
        "character" => format!("Characters/{name}"),
        "location" => format!("Locations/{name}"),
        _ => name.to_string(),
    }
}

async fn maybe_replace_kind(pool: &sqlx::PgPool, story_id: &str, kind: &str, replace: bool) {
    if replace && kind == "bible" {
        let _ = sqlx::query("DELETE FROM manuscripts WHERE story_id = $1 AND kind = 'bible'")
            .bind(story_id)
            .execute(pool)
            .await;
    }
}

/// POST /api/stories/:story_id/documents/upload — multipart files → story documents.
pub async fn upload_chapters(
    State(state): State<AppState>,
    Path(story_id): Path<String>,
    Query(query): Query<UploadQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if !loremetry_core::stories::story_exists(&state.ctx.db, &story_id).await {
        return json_error(format!("Story not found: {story_id}"));
    }

    let kind = normalize_kind(&query.kind);
    maybe_replace_kind(&state.ctx.db.pool, &story_id, kind, query.replace).await;

    let mut created = Vec::new();
    let mut errors = Vec::new();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return json_error(format!("Multipart error: {e}")),
        };

        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "untitled.md".into());
        let bytes = match field.bytes().await {
            Ok(b) => b,
            Err(e) => {
                errors.push(format!("{filename}: {e}"));
                continue;
            }
        };
        let content = String::from_utf8_lossy(&bytes).to_string();
        let title = filename
            .trim_end_matches(".md")
            .trim_end_matches(".txt")
            .to_string();

        let req = UpsertDocumentRequest {
            story_id: story_id.clone(),
            kind: kind.to_string(),
            title: title.clone(),
            path_hint: path_hint_for_kind(kind, &filename),
            content,
            id: None,
        };

        match documents::upsert_document(&state.ctx.db.pool, &req).await {
            Ok(doc) => {
                created.push(json!({
                    "id": doc.id,
                    "kind": doc.kind,
                    "path": format!("doc:{}", doc.id),
                    "title": doc.title,
                    "path_hint": doc.path_hint,
                }));
            }
            Err(e) => errors.push(format!("{filename}: {e}")),
        }
    }

    ok_json(json!({
        "success": errors.is_empty(),
        "documents": created,
        "errors": errors,
    }))
}
