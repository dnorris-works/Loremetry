use axum::extract::{Multipart, Path, State};
use axum::response::IntoResponse;
use loremetry_core::documents::{self, UpsertDocumentRequest};
use serde_json::json;

use crate::error::{json_error, ok_json};
use crate::state::AppState;

/// POST /api/stories/:story_id/documents/upload — multipart files → chapter documents.
pub async fn upload_chapters(
    State(state): State<AppState>,
    Path(story_id): Path<String>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if !loremetry_core::stories::story_exists(&state.ctx.db, &story_id).await {
        return json_error(format!("Story not found: {story_id}"));
    }

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
            kind: "chapter".into(),
            title: title.clone(),
            path_hint: filename.clone(),
            content,
            id: None,
        };

        match documents::upsert_document(&state.ctx.db.0, &req).await {
            Ok(doc) => {
                created.push(json!({
                    "id": doc.id,
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
