//! Manuscript documents stored in PostgreSQL.

use sqlx::PgPool;
use serde::{Deserialize, Serialize};

use crate::db::Db;

#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct Document {
    pub id: i64,
    pub story_id: String,
    pub kind: String,
    pub title: String,
    pub path_hint: String,
    pub content: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct DocumentMeta {
    pub id: i64,
    pub story_id: String,
    pub kind: String,
    pub title: String,
    pub path_hint: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct UpsertDocumentRequest {
    pub story_id: String,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub path_hint: String,
    pub content: String,
    #[serde(default)]
    pub id: Option<i64>,
}

#[derive(Serialize)]
pub struct DocumentsResult {
    pub success: bool,
    pub documents: Vec<DocumentMeta>,
    pub error: String,
}

#[derive(Serialize)]
pub struct DocumentResult {
    pub success: bool,
    pub document: Option<Document>,
    pub error: String,
}

pub async fn list_documents(pool: &PgPool, story_id: &str) -> Result<Vec<DocumentMeta>, String> {
    sqlx::query_as::<_, DocumentMeta>(
        "SELECT id, story_id, kind, title, path_hint, updated_at
         FROM manuscripts WHERE story_id = $1
         ORDER BY kind, path_hint, title, id",
    )
    .bind(story_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn list_chapters(pool: &PgPool, story_id: &str) -> Result<Vec<Document>, String> {
    sqlx::query_as::<_, Document>(
        "SELECT id, story_id, kind, title, path_hint, content, updated_at
         FROM manuscripts
         WHERE story_id = $1 AND kind = 'chapter'
         ORDER BY path_hint, title, id",
    )
    .bind(story_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn get_document(pool: &PgPool, id: i64) -> Result<Option<Document>, String> {
    sqlx::query_as::<_, Document>(
        "SELECT id, story_id, kind, title, path_hint, content, updated_at
         FROM manuscripts WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn upsert_document(pool: &PgPool, req: &UpsertDocumentRequest) -> Result<Document, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let path_hint = if req.path_hint.is_empty() {
        req.title.clone()
    } else {
        req.path_hint.clone()
    };
    let id = if let Some(existing_id) = req.id {
        sqlx::query(
            "UPDATE manuscripts SET kind = $1, title = $2, path_hint = $3, content = $4, updated_at = $5
             WHERE id = $6 AND story_id = $7",
        )
        .bind(&req.kind)
        .bind(&req.title)
        .bind(&path_hint)
        .bind(&req.content)
        .bind(&now)
        .bind(existing_id)
        .bind(&req.story_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
        existing_id
    } else {
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO manuscripts (story_id, kind, title, path_hint, content, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id",
        )
        .bind(&req.story_id)
        .bind(&req.kind)
        .bind(&req.title)
        .bind(&path_hint)
        .bind(&req.content)
        .bind(&now)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?
    };
    get_document(pool, id)
        .await?
        .ok_or_else(|| "Document missing after upsert".into())
}

pub async fn delete_document(pool: &PgPool, story_id: &str, id: i64) -> Result<(), String> {
    let r = sqlx::query("DELETE FROM manuscripts WHERE id = $1 AND story_id = $2")
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

pub async fn write_document_fix(
    pool: &PgPool,
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
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE manuscripts SET content = $1, updated_at = $2 WHERE id = $3")
        .bind(&doc.content)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    doc.updated_at = now;
    Ok(doc)
}

pub async fn load_bible_text(pool: &PgPool, story_id: &str) -> String {
    let mut parts = Vec::new();
    for kind in ["bible", "character", "location"] {
        if let Ok(rows) = sqlx::query_as::<_, (String, String)>(
            "SELECT title, content FROM manuscripts WHERE story_id = $1 AND kind = $2 ORDER BY path_hint, id",
        )
        .bind(story_id)
        .bind(kind)
        .fetch_all(pool)
        .await
        {
            for (title, content) in rows {
                if content.trim().is_empty() {
                    continue;
                }
                parts.push(format!("## {}\n\n{}", title, content));
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
    list_documents(&db.0, story_id).await
}

pub async fn list_chapters_db(db: &Db, story_id: &str) -> Result<Vec<Document>, String> {
    list_chapters(&db.0, story_id).await
}
