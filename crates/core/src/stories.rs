//! Story registry — stored in SQLite (no local folders).

use serde::{Deserialize, Serialize};

use crate::app_ctx::AppCtx;
use crate::db::Db;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Story {
    pub id: String,
    pub name: String,
    pub created: String,
    #[serde(default)]
    pub bible_path: String,
}

#[derive(Serialize)]
pub struct StoriesResult {
    pub success: bool,
    pub stories: Vec<Story>,
    pub error: String,
}

#[derive(Deserialize)]
pub struct InitStoryRequest {
    pub name: String,
}

#[derive(Deserialize)]
pub struct UpdateStoryRequest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub bible_path: String,
}

fn new_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{:x}", ts)
}

pub fn ensure_stories_table(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS stories (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            created    TEXT NOT NULL,
            bible_path TEXT NOT NULL DEFAULT ''
        );",
    )
    .map_err(|e| e.to_string())
}

fn load_all(conn: &rusqlite::Connection) -> Result<Vec<Story>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, created, bible_path FROM stories ORDER BY created DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Story {
                id: r.get(0)?,
                name: r.get(1)?,
                created: r.get(2)?,
                bible_path: r.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub async fn list_stories(app: AppCtx) -> StoriesResult {
    match load_all_db(&app.db) {
        Ok(stories) => StoriesResult {
            success: true,
            stories,
            error: String::new(),
        },
        Err(e) => StoriesResult {
            success: false,
            stories: vec![],
            error: e,
        },
    }
}

fn load_all_db(db: &Db) -> Result<Vec<Story>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    load_all(&conn)
}

/// Create a story by name only (no folder scaffolding).
pub async fn init_story(app: AppCtx, request: InitStoryRequest) -> StoriesResult {
    let name = request.name.trim().to_string();
    if name.is_empty() {
        return StoriesResult {
            success: false,
            stories: vec![],
            error: "Name is required".into(),
        };
    }
    let id = new_id();
    let created = chrono::Utc::now().to_rfc3339();
    {
        let conn = match app.db.0.lock() {
            Ok(c) => c,
            Err(e) => {
                return StoriesResult {
                    success: false,
                    stories: vec![],
                    error: e.to_string(),
                }
            }
        };
        if let Err(e) = conn.execute(
            "INSERT INTO stories (id, name, created, bible_path) VALUES (?1, ?2, ?3, '')",
            rusqlite::params![id, name, created],
        ) {
            return StoriesResult {
                success: false,
                stories: vec![],
                error: e.to_string(),
            };
        }
    }
    match load_all_db(&app.db) {
        Ok(stories) => StoriesResult {
            success: true,
            stories,
            error: String::new(),
        },
        Err(e) => StoriesResult {
            success: false,
            stories: vec![],
            error: e,
        },
    }
}

/// Alias for init_story (web API no longer registers external folders).
pub async fn add_story(app: AppCtx, request: InitStoryRequest) -> StoriesResult {
    init_story(app, request).await
}

pub async fn update_story(app: AppCtx, request: UpdateStoryRequest) -> StoriesResult {
    let name = request.name.trim().to_string();
    if name.is_empty() {
        return StoriesResult {
            success: false,
            stories: vec![],
            error: "Name is required".into(),
        };
    }
    {
        let conn = match app.db.0.lock() {
            Ok(c) => c,
            Err(e) => {
                return StoriesResult {
                    success: false,
                    stories: vec![],
                    error: e.to_string(),
                }
            }
        };
        let n = match conn.execute(
            "UPDATE stories SET name = ?1, bible_path = ?2 WHERE id = ?3",
            rusqlite::params![name, request.bible_path, request.id],
        ) {
            Ok(n) => n,
            Err(e) => {
                return StoriesResult {
                    success: false,
                    stories: vec![],
                    error: e.to_string(),
                }
            }
        };
        if n == 0 {
            return StoriesResult {
                success: false,
                stories: vec![],
                error: "Story not found".into(),
            };
        }
    }
    match load_all_db(&app.db) {
        Ok(stories) => StoriesResult {
            success: true,
            stories,
            error: String::new(),
        },
        Err(e) => StoriesResult {
            success: false,
            stories: vec![],
            error: e,
        },
    }
}

pub async fn delete_story(app: AppCtx, id: String) -> StoriesResult {
    {
        let conn = match app.db.0.lock() {
            Ok(c) => c,
            Err(e) => {
                return StoriesResult {
                    success: false,
                    stories: vec![],
                    error: e.to_string(),
                }
            }
        };
        let _ = conn.execute("DELETE FROM stories WHERE id = ?1", rusqlite::params![id]);
        let _ = conn.execute(
            "DELETE FROM manuscripts WHERE story_id = ?1",
            rusqlite::params![id],
        );
    }
    match load_all_db(&app.db) {
        Ok(stories) => StoriesResult {
            success: true,
            stories,
            error: String::new(),
        },
        Err(e) => StoriesResult {
            success: false,
            stories: vec![],
            error: e,
        },
    }
}

pub fn story_exists(db: &Db, id: &str) -> bool {
    let Ok(conn) = db.0.lock() else {
        return false;
    };
    conn.query_row(
        "SELECT 1 FROM stories WHERE id = ?1",
        rusqlite::params![id],
        |_| Ok(()),
    )
    .is_ok()
}
