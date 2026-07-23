// series.rs — Series management (groups stories into reading order)

use sqlx::PgPool;
use serde::{Deserialize, Serialize};
use crate::app_ctx::AppCtx;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Series {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub bible_path: String,
    pub books: Vec<SeriesBook>,
}

#[derive(Serialize, Deserialize, Clone, Debug, sqlx::FromRow)]
pub struct SeriesBook {
    pub story_id: String,
    pub story_name: String,
    pub book_order: i64,
}

#[derive(Serialize)]
pub struct SeriesResult {
    pub success: bool,
    pub series: Vec<Series>,
    pub error: String,
}

#[derive(Deserialize)]
pub struct CreateSeriesRequest {
    pub name: String,
    pub books: Vec<SeriesBookInput>,
}

#[derive(Deserialize)]
pub struct UpdateSeriesRequest {
    pub id: i64,
    pub name: String,
    pub books: Vec<SeriesBookInput>,
    #[serde(default)]
    pub bible_path: String,
}

#[derive(Deserialize, Clone)]
pub struct SeriesBookInput {
    pub story_id: String,
    pub story_name: String,
    pub book_order: i64,
}

pub async fn list_series(app: AppCtx) -> SeriesResult {
    let pool = &app.db.pool;
    let series_rows: Vec<(i64, String, String, String)> = match sqlx::query_as(
        r#"SELECT id, name, created_at, COALESCE(bible_path, '') FROM "series" ORDER BY name"#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            return SeriesResult {
                success: false,
                series: Vec::new(),
                error: e.to_string(),
            }
        }
    };

    let mut series_list = Vec::new();
    for (id, name, created_at, bible_path) in series_rows {
        let books = load_series_books(pool, id).await;
        series_list.push(Series {
            id,
            name,
            created_at,
            bible_path,
            books,
        });
    }

    SeriesResult {
        success: true,
        series: series_list,
        error: String::new(),
    }
}

pub async fn create_series(app: AppCtx, request: CreateSeriesRequest) -> SeriesResult {
    let pool = &app.db.pool;
    let name = request.name.trim();
    if name.is_empty() {
        return SeriesResult {
            success: false,
            series: Vec::new(),
            error: "Series name is required.".to_string(),
        };
    }

    let now = chrono::Utc::now().to_rfc3339();
    let series_id: i64 = match sqlx::query_scalar(
        r#"INSERT INTO "series" (name, created_at) VALUES ($1, $2) RETURNING id"#,
    )
    .bind(name)
    .bind(&now)
    .fetch_one(pool)
    .await
    {
        Ok(id) => id,
        Err(e) => {
            return SeriesResult {
                success: false,
                series: Vec::new(),
                error: format!("Could not create series: {}", e),
            }
        }
    };

    if let Err(e) = save_series_books(pool, series_id, &request.books).await {
        return SeriesResult {
            success: false,
            series: Vec::new(),
            error: e,
        };
    }

    list_series(app).await
}

pub async fn update_series(app: AppCtx, request: UpdateSeriesRequest) -> SeriesResult {
    let pool = &app.db.pool;
    let name = request.name.trim();
    if name.is_empty() {
        return SeriesResult {
            success: false,
            series: Vec::new(),
            error: "Series name is required.".to_string(),
        };
    }

    if let Err(e) = sqlx::query(
        r#"UPDATE "series" SET name = $1, bible_path = $2 WHERE id = $3"#,
    )
    .bind(name)
    .bind(&request.bible_path)
    .bind(request.id)
    .execute(pool)
    .await
    {
        return SeriesResult {
            success: false,
            series: Vec::new(),
            error: format!("Could not update series: {}", e),
        };
    }

    if let Err(e) = save_series_books(pool, request.id, &request.books).await {
        return SeriesResult {
            success: false,
            series: Vec::new(),
            error: e,
        };
    }

    list_series(app).await
}

pub async fn delete_series(app: AppCtx, id: i64) -> SeriesResult {
    let pool = &app.db.pool;
    let _ = sqlx::query("DELETE FROM series_books WHERE series_id = $1")
        .bind(id)
        .execute(pool)
        .await;
    let _ = sqlx::query(r#"DELETE FROM "series" WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await;
    list_series(app).await
}

async fn load_series_books(pool: &PgPool, series_id: i64) -> Vec<SeriesBook> {
    sqlx::query_as::<_, SeriesBook>(
        "SELECT story_id, story_name, book_order FROM series_books WHERE series_id = $1 ORDER BY book_order",
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

async fn save_series_books(pool: &PgPool, series_id: i64, books: &[SeriesBookInput]) -> Result<(), String> {
    sqlx::query("DELETE FROM series_books WHERE series_id = $1")
        .bind(series_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    for book in books {
        sqlx::query(
            "INSERT INTO series_books (series_id, story_id, story_name, book_order) VALUES ($1, $2, $3, $4)",
        )
        .bind(series_id)
        .bind(&book.story_id)
        .bind(&book.story_name)
        .bind(book.book_order)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
