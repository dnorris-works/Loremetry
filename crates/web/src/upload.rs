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
    let normalized = filename.replace('\\', "/");
    let name = if normalized.is_empty() {
        "untitled.md".to_string()
    } else {
        normalized
    };
    match kind {
        "bible" => {
            let base = basename(&name);
            format!("Bible/{base}")
        }
        "character" => {
            let base = basename(&name);
            format!("Characters/{base}")
        }
        "location" => {
            let base = basename(&name);
            format!("Locations/{base}")
        }
        _ => name,
    }
}

fn basename(path: &str) -> String {
    path.rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn title_from_path(path: &str) -> String {
    let base = basename(path);
    base.trim_end_matches(".md")
        .trim_end_matches(".txt")
        .trim_end_matches(".markdown")
        .to_string()
}

fn is_manuscript_filename(path: &str) -> bool {
    let lower = basename(path).to_lowercase();
    lower.ends_with(".md") || lower.ends_with(".txt") || lower.ends_with(".markdown")
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

        if kind == "chapter" && !is_manuscript_filename(&filename) {
            continue;
        }

        let bytes = match field.bytes().await {
            Ok(b) => b,
            Err(e) => {
                errors.push(format!("{filename}: {e}"));
                continue;
            }
        };
        let content = String::from_utf8_lossy(&bytes).to_string();
        let title = title_from_path(&filename);

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
