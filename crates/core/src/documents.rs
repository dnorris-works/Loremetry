//! Manuscript documents — API layer over `story_assets`.

use serde::{Deserialize, Serialize};

use crate::assets::{
    self, kind_from_slot, path_hint_for, slot_from_kind, StoryAsset,
};
use crate::db::Db;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Document {
    pub id:         i64,
    pub story_id:   String,
    pub kind:       String,
    pub title:      String,
    pub path_hint:  String,
    pub content:    String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DocumentMeta {
    pub id:         i64,
    pub story_id:   String,
    pub kind:       String,
    pub title:      String,
    pub path_hint:  String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct UpsertDocumentRequest {
    pub story_id: String,
    pub kind:     String,
    #[serde(default)]
    pub path_hint: String,
    pub title:    String,
    pub content:  String,
    #[serde(default)]
    pub id: Option<i64>,
}

#[derive(Serialize)]
pub struct DocumentsResult {
    pub success:   bool,
    pub documents: Vec<DocumentMeta>,
    pub error:     String,
}

#[derive(Serialize)]
pub struct DocumentResult {
    pub success:  bool,
    pub document: Option<Document>,
    pub error:    String,
}

fn asset_to_document(asset: &StoryAsset) -> Document {
    Document {
        id:         asset.id,
        story_id:   asset.story_id.clone(),
        kind:       kind_from_slot(&asset.slot).to_string(),
        title:      asset.title.clone(),
        path_hint:  path_hint_for(&asset.slot, &asset.filename),
        content:    asset.content.clone(),
        updated_at: asset.updated_at.clone(),
    }
}

fn asset_to_meta(asset: &StoryAsset) -> DocumentMeta {
    DocumentMeta {
        id:         asset.id,
        story_id:   asset.story_id.clone(),
        kind:       kind_from_slot(&asset.slot).to_string(),
        title:      asset.title.clone(),
        path_hint:  path_hint_for(&asset.slot, &asset.filename),
        updated_at: asset.updated_at.clone(),
    }
}

pub async fn list_documents(pool: &sqlx::PgPool, story_id: &str) -> Result<Vec<DocumentMeta>, String> {
    let assets = assets::list_assets(pool, story_id).await?;
    Ok(assets.iter().map(asset_to_meta).collect())
}

pub async fn list_chapters(pool: &sqlx::PgPool, story_id: &str) -> Result<Vec<Document>, String> {
    let assets = assets::list_assets_by_slot(pool, story_id, assets::SLOT_MANUSCRIPT).await?;
    Ok(assets.iter().map(asset_to_document).collect())
}

pub async fn get_document(pool: &sqlx::PgPool, id: i64) -> Result<Option<Document>, String> {
    let asset = assets::get_asset(pool, id).await?;
    Ok(asset.as_ref().map(asset_to_document))
}

pub async fn upsert_document(pool: &sqlx::PgPool, req: &UpsertDocumentRequest) -> Result<Document, String> {
    let slot = slot_from_kind(&req.kind);
    let filename = if !req.path_hint.is_empty() {
        assets::sanitize_filename(&req.path_hint)
    } else if !req.title.is_empty() {
        format!("{}.md", req.title.replace('/', "_"))
    } else {
        "untitled.md".to_string()
    };

    if let Some(existing_id) = req.id {
        let asset = assets::upsert_asset_by_id(
            pool,
            existing_id,
            &req.story_id,
            slot,
            &req.title,
            &filename,
            &req.content,
            "md",
        )
        .await?;
        return Ok(asset_to_document(&asset));
    }

    let (asset, _) = assets::merge_asset(
        pool,
        &req.story_id,
        slot,
        &filename,
        &req.title,
        &req.content,
        "md",
        None,
    )
    .await?;
    Ok(asset_to_document(&asset))
}

pub async fn delete_document(pool: &sqlx::PgPool, story_id: &str, id: i64) -> Result<(), String> {
    assets::delete_asset(pool, story_id, id).await
}

pub async fn write_document_fix(
    pool: &sqlx::PgPool,
    id: i64,
    old_text: &str,
    new_text: &str,
) -> Result<Document, String> {
    let mut doc = get_document(pool, id)
        .await?
        .ok_or_else(|| "Document not found".to_string())?;
    if !doc.content.contains(old_text) {
        return Err("Old text not found in document".into());
    }
    doc.content = doc.content.replacen(old_text, new_text, 1);
    let slot = slot_from_kind(&doc.kind);
    let filename = assets::sanitize_filename(&doc.path_hint);
    let asset = assets::upsert_asset_by_id(
        pool,
        id,
        &doc.story_id,
        slot,
        &doc.title,
        &filename,
        &doc.content,
        "md",
    )
    .await?;
    Ok(asset_to_document(&asset))
}

pub async fn load_bible_text(pool: &sqlx::PgPool, story_id: &str) -> String {
    let mut parts = Vec::new();
    for slot in [assets::SLOT_BIBLE, assets::SLOT_CHARACTER, assets::SLOT_LOCATION] {
        if let Ok(rows) = assets::list_assets_by_slot(pool, story_id, slot).await {
            for asset in rows {
                if asset.content.trim().is_empty() {
                    continue;
                }
                parts.push(format!("## {}\n\n{}", asset.title, asset.content));
            }
        }
    }
    parts.join("\n\n")
}

pub fn chapter_display_name(doc: &Document) -> String {
    if !doc.path_hint.is_empty() {
        doc.path_hint.clone()
    } else if !doc.title.is_empty() {
        doc.title.clone()
    } else {
        format!("chapter-{}.md", doc.id)
    }
}

pub async fn list_documents_db(db: &Db, story_id: &str) -> Result<Vec<DocumentMeta>, String> {
    list_documents(&db.pool, story_id).await
}

pub async fn list_chapters_db(db: &Db, story_id: &str) -> Result<Vec<Document>, String> {
    list_chapters(&db.pool, story_id).await
}
