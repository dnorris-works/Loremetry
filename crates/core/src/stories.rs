//! Story registry — stored in PostgreSQL.

use sqlx::PgPool;
use serde::{Deserialize, Serialize};

use crate::app_ctx::AppCtx;
use crate::db::Db;

#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
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

async fn load_all(pool: &PgPool) -> Result<Vec<Story>, String> {
    sqlx::query_as::<_, Story>(
        "SELECT id, name, created, bible_path FROM stories ORDER BY created DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn list_stories(app: AppCtx) -> StoriesResult {
    match load_all(&app.db.0).await {
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
    if let Err(e) = sqlx::query(
        "INSERT INTO stories (id, name, created, bible_path) VALUES ($1, $2, $3, '')",
    )
    .bind(&id)
    .bind(&name)
    .bind(&created)
    .execute(&app.db.0)
    .await
    {
        return StoriesResult {
            success: false,
            stories: vec![],
            error: e.to_string(),
        };
    }
    match load_all(&app.db.0).await {
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
    let n = match sqlx::query(
        "UPDATE stories SET name = $1, bible_path = $2 WHERE id = $3",
    )
    .bind(&name)
    .bind(&request.bible_path)
    .bind(&request.id)
    .execute(&app.db.0)
    .await
    {
        Ok(r) => r.rows_affected(),
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
    match load_all(&app.db.0).await {
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
    let _ = sqlx::query("DELETE FROM stories WHERE id = $1")
        .bind(&id)
        .execute(&app.db.0)
        .await;
    let _ = sqlx::query("DELETE FROM manuscripts WHERE story_id = $1")
        .bind(&id)
        .execute(&app.db.0)
        .await;
    match load_all(&app.db.0).await {
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

pub async fn story_exists(db: &Db, id: &str) -> bool {
    sqlx::query_scalar::<_, i32>("SELECT 1 FROM stories WHERE id = $1 LIMIT 1")
        .bind(id)
        .fetch_optional(&db.0)
        .await
        .ok()
        .flatten()
        .is_some()
}
