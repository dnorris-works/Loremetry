use axum::extract::{Multipart, Path, Query, State};
use axum::response::IntoResponse;
use loremetry_core::assets::{self, MergeAction};
use loremetry_core::asset_export::{read_zip_entries, read_zip_manifest};
use loremetry_core::docx::{convert_docx_to_markdown, docx_to_md_filename};
use serde::Deserialize;
use serde_json::json;

use crate::error::{json_error, ok_json};
use crate::state::AppState;

#[derive(Deserialize, Default)]
pub struct UploadQuery {
    /// chapter | bible | character | location (maps to story_assets slots)
    #[serde(default)]
    pub kind: String,
    /// When true, delete existing bible slot assets before uploading.
    #[serde(default)]
    pub replace: bool,
}

fn default_slot(kind: &str) -> &'static str {
    assets::slot_from_kind(kind)
}

fn infer_slot_from_zip_path(path: &str, default: &str) -> (String, String) {
    let normalized = path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return (default.to_string(), "untitled.md".to_string());
    }
    let folder = parts[0].to_lowercase();
    let filename = parts.last().map_or("untitled.md", |s| *s).to_string();
    let slot = match folder.as_str() {
        "manuscript" => assets::SLOT_MANUSCRIPT,
        "bible" => assets::SLOT_BIBLE,
        "characters" | "character" => assets::SLOT_CHARACTER,
        "locations" | "location" => assets::SLOT_LOCATION,
        "reports" => return (String::new(), filename), // skip reports on re-import
        _ => default,
    };
    (slot.to_string(), filename)
}

struct ProcessedFile {
    slot: String,
    filename: String,
    title: String,
    content: String,
    source_format: String,
}

fn process_bytes(
    raw_path: &str,
    bytes: &[u8],
    slot: &str,
) -> Result<ProcessedFile, String> {
    let filename = assets::sanitize_filename(raw_path);
    let lower = filename.to_lowercase();

    if lower.ends_with(".docx") {
        let content = convert_docx_to_markdown(bytes)?;
        let md_name = docx_to_md_filename(&filename);
        let title = assets::title_from_filename(&md_name);
        return Ok(ProcessedFile {
            slot: slot.to_string(),
            filename: md_name,
            title,
            content,
            source_format: "docx".to_string(),
        });
    }

    if lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".txt") {
        let content = String::from_utf8_lossy(bytes).to_string();
        let md_name = if lower.ends_with(".txt") {
            format!("{}.md", filename.trim_end_matches(".txt"))
        } else {
            filename.clone()
        };
        let title = assets::title_from_filename(&md_name);
        return Ok(ProcessedFile {
            slot: slot.to_string(),
            filename: md_name,
            title,
            content,
            source_format: "md".to_string(),
        });
    }

    Err(format!("{filename}: unsupported format (use .md or .docx)"))
}

async fn process_zip_upload(
    pool: &sqlx::PgPool,
    story_id: &str,
    bytes: &[u8],
    default_slot: &str,
    replace_bible: bool,
) -> (Vec<serde_json::Value>, Vec<String>, usize, usize) {
    let mut created = Vec::new();
    let mut errors = Vec::new();
    let mut updated = 0usize;
    let mut skipped = 0usize;

    if replace_bible {
        let _ = assets::delete_assets_by_slot(pool, story_id, assets::SLOT_BIBLE).await;
    }

    let manifest = read_zip_manifest(bytes);
    let manifest_map: std::collections::HashMap<String, (String, i32)> = manifest
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.filename.to_lowercase(), (e.slot, e.sort_order)))
        .collect();

    let entries = match read_zip_entries(bytes) {
        Ok(e) => e,
        Err(e) => {
            errors.push(e);
            return (created, errors, updated, skipped);
        }
    };

    for (path, data) in entries {
        let (slot, filename) = if let Some((slot, order)) = manifest_map.get(&assets::sanitize_filename(&path).to_lowercase()) {
            (slot.clone(), assets::sanitize_filename(&path))
        } else {
            infer_slot_from_zip_path(&path, default_slot)
        };
        if slot.is_empty() {
            continue; // Reports/ etc.
        }
        if !assets::is_allowed_upload_filename(&filename) {
            errors.push(format!("{path}: unsupported format"));
            continue;
        }
        match process_bytes(&filename, &data, &slot) {
            Ok(pf) => {
                let order = manifest_map
                    .get(&pf.filename.to_lowercase())
                    .map(|(_, o)| *o);
                match assets::merge_asset(
                    pool,
                    story_id,
                    &pf.slot,
                    &pf.filename,
                    &pf.title,
                    &pf.content,
                    &pf.source_format,
                    order,
                )
                .await
                {
                    Ok((asset, action)) => {
                        match action {
                            MergeAction::Created => {
                                created.push(json!({
                                    "id": asset.id,
                                    "kind": assets::kind_from_slot(&asset.slot),
                                    "path_hint": assets::path_hint_for(&asset.slot, &asset.filename),
                                    "title": asset.title,
                                    "action": "created",
                                }));
                            }
                            MergeAction::Updated => {
                                updated += 1;
                                created.push(json!({
                                    "id": asset.id,
                                    "kind": assets::kind_from_slot(&asset.slot),
                                    "path_hint": assets::path_hint_for(&asset.slot, &asset.filename),
                                    "title": asset.title,
                                    "action": "updated",
                                }));
                            }
                            MergeAction::Skipped => skipped += 1,
                        }
                    }
                    Err(e) => errors.push(format!("{path}: {e}")),
                }
            }
            Err(e) => errors.push(e),
        }
    }

    (created, errors, updated, skipped)
}

/// POST /api/stories/:story_id/documents/upload — multipart files → story_assets (merge by hash).
pub async fn upload_chapters(
    State(state): State<AppState>,
    Path(story_id): Path<String>,
    Query(query): Query<UploadQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if !loremetry_core::stories::story_exists(&state.ctx.db, &story_id).await {
        return json_error(format!("Story not found: {story_id}"));
    }

    let default_slot = default_slot(&query.kind);
    if query.replace && default_slot == assets::SLOT_BIBLE {
        let _ = assets::delete_assets_by_slot(&state.ctx.db.pool, &story_id, assets::SLOT_BIBLE).await;
    }

    let mut documents = Vec::new();
    let mut errors = Vec::new();
    let mut updated = 0usize;
    let mut skipped = 0usize;

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

        if assets::is_zip_filename(&filename) {
            let (zip_docs, zip_errors, zip_updated, zip_skipped) = process_zip_upload(
                &state.ctx.db.pool,
                &story_id,
                &bytes,
                default_slot,
                false,
            )
            .await;
            documents.extend(zip_docs);
            errors.extend(zip_errors);
            updated += zip_updated;
            skipped += zip_skipped;
            continue;
        }

        if !assets::is_allowed_upload_filename(&filename) {
            errors.push(format!("{filename}: unsupported format (use .md, .txt, or .docx)"));
            continue;
        }

        let slot = default_slot;
        match process_bytes(&filename, &bytes, slot) {
            Ok(pf) => {
                match assets::merge_asset(
                    &state.ctx.db.pool,
                    &story_id,
                    &pf.slot,
                    &pf.filename,
                    &pf.title,
                    &pf.content,
                    &pf.source_format,
                    None,
                )
                .await
                {
                    Ok((asset, action)) => match action {
                        MergeAction::Created => {
                            documents.push(json!({
                                "id": asset.id,
                                "kind": assets::kind_from_slot(&asset.slot),
                                "path_hint": assets::path_hint_for(&asset.slot, &asset.filename),
                                "title": asset.title,
                                "action": "created",
                            }));
                        }
                        MergeAction::Updated => {
                            updated += 1;
                            documents.push(json!({
                                "id": asset.id,
                                "kind": assets::kind_from_slot(&asset.slot),
                                "path_hint": assets::path_hint_for(&asset.slot, &asset.filename),
                                "title": asset.title,
                                "action": "updated",
                            }));
                        }
                        MergeAction::Skipped => skipped += 1,
                    },
                    Err(e) => errors.push(format!("{filename}: {e}")),
                }
            }
            Err(e) => errors.push(e),
        }
    }

    ok_json(json!({
        "success": errors.is_empty(),
        "documents": documents,
        "updated": updated,
        "skipped": skipped,
        "errors": errors,
    }))
}
