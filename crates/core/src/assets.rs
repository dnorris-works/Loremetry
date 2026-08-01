//! story_assets — slot-based author content in Postgres.

use sha2::{Digest, Sha256};
use sqlx::PgPool;

pub const SLOT_MANUSCRIPT: &str = "manuscript";
pub const SLOT_BIBLE: &str = "bible";
pub const SLOT_CHARACTER: &str = "character";
pub const SLOT_LOCATION: &str = "location";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoryAsset {
    pub id:            i64,
    pub story_id:      String,
    pub slot:          String,
    pub title:         String,
    pub filename:      String,
    pub sort_order:    i32,
    pub content:       String,
    pub content_hash:  String,
    pub source_format: String,
    pub created_at:    String,
    pub updated_at:    String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeAction {
    Created,
    Updated,
    Skipped,
}

pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn kind_from_slot(slot: &str) -> &'static str {
    match slot {
        SLOT_BIBLE => "bible",
        SLOT_CHARACTER => "character",
        SLOT_LOCATION => "location",
        _ => "chapter",
    }
}

pub fn slot_from_kind(kind: &str) -> &'static str {
    match kind.trim().to_lowercase().as_str() {
        "bible" => SLOT_BIBLE,
        "character" | "characters" => SLOT_CHARACTER,
        "location" | "locations" => SLOT_LOCATION,
        "manuscript" => SLOT_MANUSCRIPT,
        _ => SLOT_MANUSCRIPT,
    }
}

pub fn slot_folder(slot: &str) -> &'static str {
    match slot {
        SLOT_BIBLE => "Bible",
        SLOT_CHARACTER => "Characters",
        SLOT_LOCATION => "Locations",
        _ => "Manuscript",
    }
}

pub fn path_hint_for(slot: &str, filename: &str) -> String {
    let name = sanitize_filename(filename);
    match slot {
        SLOT_BIBLE => format!("Bible/{name}"),
        SLOT_CHARACTER => format!("Characters/{name}"),
        SLOT_LOCATION => format!("Locations/{name}"),
        _ => name,
    }
}

pub fn title_from_filename(filename: &str) -> String {
    let base = sanitize_filename(filename);
    base.trim_end_matches(".md")
        .trim_end_matches(".markdown")
        .trim_end_matches(".txt")
        .trim_end_matches(".docx")
        .to_string()
}

pub fn sanitize_filename(name: &str) -> String {
    let normalized = name.replace('\\', "/");
    normalized
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(&normalized)
        .to_string()
}

pub fn is_allowed_upload_filename(filename: &str) -> bool {
    let lower = sanitize_filename(filename).to_lowercase();
    lower.ends_with(".md")
        || lower.ends_with(".markdown")
        || lower.ends_with(".txt")
        || lower.ends_with(".docx")
}

pub fn is_zip_filename(filename: &str) -> bool {
    sanitize_filename(filename).to_lowercase().ends_with(".zip")
}

pub async fn list_assets(pool: &PgPool, story_id: &str) -> Result<Vec<StoryAsset>, String> {
    sqlx::query_as::<_, StoryAsset>(
        "SELECT id, story_id, slot, title, filename, sort_order, content, content_hash, source_format, created_at, updated_at
         FROM story_assets WHERE story_id = $1
         ORDER BY slot, sort_order, filename, id",
    )
    .bind(story_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn list_assets_by_slot(pool: &PgPool, story_id: &str, slot: &str) -> Result<Vec<StoryAsset>, String> {
    sqlx::query_as::<_, StoryAsset>(
        "SELECT id, story_id, slot, title, filename, sort_order, content, content_hash, source_format, created_at, updated_at
         FROM story_assets WHERE story_id = $1 AND slot = $2
         ORDER BY sort_order, filename, id",
    )
    .bind(story_id)
    .bind(slot)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn get_asset(pool: &PgPool, id: i64) -> Result<Option<StoryAsset>, String> {
    sqlx::query_as::<_, StoryAsset>(
        "SELECT id, story_id, slot, title, filename, sort_order, content, content_hash, source_format, created_at, updated_at
         FROM story_assets WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn find_by_filename_ci(
    pool: &PgPool,
    story_id: &str,
    slot: &str,
    filename: &str,
) -> Result<Option<StoryAsset>, String> {
    let name = sanitize_filename(filename);
    sqlx::query_as::<_, StoryAsset>(
        "SELECT id, story_id, slot, title, filename, sort_order, content, content_hash, source_format, created_at, updated_at
         FROM story_assets
         WHERE story_id = $1 AND slot = $2 AND lower(filename) = lower($3)",
    )
    .bind(story_id)
    .bind(slot)
    .bind(&name)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn delete_asset(pool: &PgPool, story_id: &str, id: i64) -> Result<(), String> {
    let r = sqlx::query("DELETE FROM story_assets WHERE id = $1 AND story_id = $2")
        .bind(id)
        .bind(story_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    if r.rows_affected() == 0 {
        return Err("Document not found".into());
    }
    Ok(())
}

pub async fn delete_assets_by_slot(pool: &PgPool, story_id: &str, slot: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM story_assets WHERE story_id = $1 AND slot = $2")
        .bind(story_id)
        .bind(slot)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn delete_all_assets(pool: &PgPool, story_id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM story_assets WHERE story_id = $1")
        .bind(story_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn merge_asset(
    pool: &PgPool,
    story_id: &str,
    slot: &str,
    filename: &str,
    title: &str,
    content: &str,
    source_format: &str,
    sort_order: Option<i32>,
) -> Result<(StoryAsset, MergeAction), String> {
    let name = sanitize_filename(filename);
    let hash = hash_content(content);
    let now = chrono::Utc::now().to_rfc3339();
    let title = if title.is_empty() {
        title_from_filename(&name)
    } else {
        title.to_string()
    };

    if let Some(existing) = find_by_filename_ci(pool, story_id, slot, &name).await? {
        if existing.content_hash == hash {
            return Ok((existing, MergeAction::Skipped));
        }
        sqlx::query(
            "UPDATE story_assets SET title = $1, content = $2, content_hash = $3, source_format = $4, updated_at = $5
             WHERE id = $6",
        )
        .bind(&title)
        .bind(content)
        .bind(&hash)
        .bind(source_format)
        .bind(&now)
        .bind(existing.id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
        let updated = get_asset(pool, existing.id)
            .await?
            .ok_or_else(|| "Asset missing after update".to_string())?;
        return Ok((updated, MergeAction::Updated));
    }

    let order = sort_order.unwrap_or(0);
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO story_assets (story_id, slot, title, filename, sort_order, content, content_hash, source_format, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING id",
    )
    .bind(story_id)
    .bind(slot)
    .bind(&title)
    .bind(&name)
    .bind(order)
    .bind(content)
    .bind(&hash)
    .bind(source_format)
    .bind(&now)
    .bind(&now)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    let created = get_asset(pool, id)
        .await?
        .ok_or_else(|| "Asset missing after insert".to_string())?;
    Ok((created, MergeAction::Created))
}

pub async fn upsert_asset_by_id(
    pool: &PgPool,
    id: i64,
    story_id: &str,
    slot: &str,
    title: &str,
    filename: &str,
    content: &str,
    source_format: &str,
) -> Result<StoryAsset, String> {
    let hash = hash_content(content);
    let now = chrono::Utc::now().to_rfc3339();
    let name = sanitize_filename(filename);
    sqlx::query(
        "UPDATE story_assets SET slot = $1, title = $2, filename = $3, content = $4, content_hash = $5, source_format = $6, updated_at = $7
         WHERE id = $8 AND story_id = $9",
    )
    .bind(slot)
    .bind(title)
    .bind(&name)
    .bind(content)
    .bind(&hash)
    .bind(source_format)
    .bind(&now)
    .bind(id)
    .bind(story_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    get_asset(pool, id)
        .await?
        .ok_or_else(|| "Document not found".into())
}

pub fn asset_display_name(asset: &StoryAsset) -> String {
    path_hint_for(&asset.slot, &asset.filename)
}
