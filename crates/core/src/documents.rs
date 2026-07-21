//! Manuscript documents stored in SQLite (replaces filesystem chapters).

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db::Db;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Document {
    pub id: i64,
    pub story_id: String,
    pub kind: String,
    pub title: String,
    pub path_hint: String,
    pub content: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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

pub fn ensure_documents_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS manuscripts (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            story_id   TEXT NOT NULL,
            kind       TEXT NOT NULL DEFAULT 'chapter',
            title      TEXT NOT NULL DEFAULT '',
            path_hint  TEXT NOT NULL DEFAULT '',
            content    TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_manuscripts_story ON manuscripts(story_id, kind);
        CREATE INDEX IF NOT EXISTS idx_manuscripts_path ON manuscripts(story_id, path_hint);"
    )
    .map_err(|e| e.to_string())
}

pub fn list_documents(conn: &Connection, story_id: &str) -> Result<Vec<DocumentMeta>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, story_id, kind, title, path_hint, updated_at
             FROM manuscripts WHERE story_id = ?1
             ORDER BY kind, path_hint, title, id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![story_id], |r| {
            Ok(DocumentMeta {
                id: r.get(0)?,
                story_id: r.get(1)?,
                kind: r.get(2)?,
                title: r.get(3)?,
                path_hint: r.get(4)?,
                updated_at: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn list_chapters(conn: &Connection, story_id: &str) -> Result<Vec<Document>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, story_id, kind, title, path_hint, content, updated_at
             FROM manuscripts
             WHERE story_id = ?1 AND kind = 'chapter'
             ORDER BY path_hint, title, id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![story_id], |r| {
            Ok(Document {
                id: r.get(0)?,
                story_id: r.get(1)?,
                kind: r.get(2)?,
                title: r.get(3)?,
                path_hint: r.get(4)?,
                content: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn get_document(conn: &Connection, id: i64) -> Result<Option<Document>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, story_id, kind, title, path_hint, content, updated_at
             FROM manuscripts WHERE id = ?1",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map(params![id], |r| {
            Ok(Document {
                id: r.get(0)?,
                story_id: r.get(1)?,
                kind: r.get(2)?,
                title: r.get(3)?,
                path_hint: r.get(4)?,
                content: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    match rows.next() {
        Some(Ok(d)) => Ok(Some(d)),
        Some(Err(e)) => Err(e.to_string()),
        None => Ok(None),
    }
}

pub fn upsert_document(conn: &Connection, req: &UpsertDocumentRequest) -> Result<Document, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let path_hint = if req.path_hint.is_empty() {
        req.title.clone()
    } else {
        req.path_hint.clone()
    };
    let id = if let Some(existing_id) = req.id {
        conn.execute(
            "UPDATE manuscripts SET kind = ?1, title = ?2, path_hint = ?3, content = ?4, updated_at = ?5
             WHERE id = ?6 AND story_id = ?7",
            params![
                req.kind,
                req.title,
                path_hint,
                req.content,
                now,
                existing_id,
                req.story_id
            ],
        )
        .map_err(|e| e.to_string())?;
        existing_id
    } else {
        conn.execute(
            "INSERT INTO manuscripts (story_id, kind, title, path_hint, content, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![req.story_id, req.kind, req.title, path_hint, req.content, now],
        )
        .map_err(|e| e.to_string())?;
        conn.last_insert_rowid()
    };
    get_document(conn, id)?.ok_or_else(|| "Document missing after upsert".into())
}

pub fn delete_document(conn: &Connection, story_id: &str, id: i64) -> Result<(), String> {
    let n = conn
        .execute(
            "DELETE FROM manuscripts WHERE id = ?1 AND story_id = ?2",
            params![id, story_id],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("Document not found".into());
    }
    Ok(())
}

pub fn write_document_fix(
    conn: &Connection,
    id: i64,
    old_text: &str,
    new_text: &str,
) -> Result<Document, String> {
    let mut doc = get_document(conn, id)?.ok_or_else(|| "Document not found".to_string())?;
    if !doc.content.contains(old_text) {
        return Err("Old text not found in document".into());
    }
    doc.content = doc.content.replacen(old_text, new_text, 1);
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE manuscripts SET content = ?1, updated_at = ?2 WHERE id = ?3",
        params![doc.content, now, id],
    )
    .map_err(|e| e.to_string())?;
    doc.updated_at = now;
    Ok(doc)
}

pub fn load_bible_text(conn: &Connection, story_id: &str) -> String {
    let mut parts = Vec::new();
    for kind in ["bible", "character", "location"] {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT title, content FROM manuscripts WHERE story_id = ?1 AND kind = ?2 ORDER BY path_hint, id",
        ) {
            if let Ok(rows) = stmt.query_map(params![story_id, kind], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            }) {
                for row in rows.flatten() {
                    let (title, content) = row;
                    if content.trim().is_empty() {
                        continue;
                    }
                    parts.push(format!("## {}\n\n{}", title, content));
                }
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

/// Convenience wrappers using Db mutex.
pub fn list_documents_db(db: &Db, story_id: &str) -> Result<Vec<DocumentMeta>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    list_documents(&conn, story_id)
}

pub fn list_chapters_db(db: &Db, story_id: &str) -> Result<Vec<Document>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    list_chapters(&conn, story_id)
}
