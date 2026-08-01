//! Postgres-backed job queue for long-running analysis pipelines.

use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::analysis::pipeline;
use crate::analysis::{AnalyzeStoryRequest, GenreResult};
use crate::app_ctx::AppCtx;
use crate::canopy::{self, MarketIntelRequest};
use crate::cancel::{cancel_operation, is_cancelled, reset as reset_cancel};

pub const JOB_ANALYZE_STORY: &str = "analyze_story";
pub const JOB_CRAFT_PIPELINE: &str = "craft_pipeline";
pub const JOB_MARKET_INTEL: &str = "market_intel";

#[derive(Debug, Clone, Serialize)]
pub struct EnqueueResult {
    pub job_id: Uuid,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobRecord {
    pub id: Uuid,
    pub job_type: String,
    pub story_id: String,
    pub user_id: Uuid,
    pub status: String,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub cancel_requested: bool,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClaimedJob {
    pub id: Uuid,
    pub job_type: String,
    pub story_id: String,
    pub user_id: Uuid,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobEvent {
    pub id: i64,
    pub channel: String,
    pub message: String,
}

pub async fn enqueue(
    pool: &PgPool,
    job_type: &str,
    story_id: &str,
    user_id: Uuid,
    payload: Value,
) -> Result<EnqueueResult, String> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO jobs (id, job_type, story_id, user_id, status, payload)
         VALUES ($1, $2, $3, $4, 'pending', $5)",
    )
    .bind(id)
    .bind(job_type)
    .bind(story_id)
    .bind(user_id)
    .bind(payload)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(EnqueueResult {
        job_id: id,
        status: "pending".into(),
    })
}

pub async fn claim_next(pool: &PgPool) -> Option<ClaimedJob> {
    let row = sqlx::query(
        "UPDATE jobs
         SET status = 'running', started_at = now()
         WHERE id = (
             SELECT id FROM jobs
             WHERE status = 'pending'
             ORDER BY created_at
             FOR UPDATE SKIP LOCKED
             LIMIT 1
         )
         RETURNING id, job_type, story_id, user_id, payload",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    row.and_then(|r| {
        let id: Uuid = r.try_get(0).ok()?;
        let job_type: String = r.try_get(1).ok()?;
        let story_id: String = r.try_get(2).ok()?;
        let user_id: Uuid = r.try_get(3).ok()?;
        let payload: Value = r.try_get(4).ok()?;
        Some(ClaimedJob {
            id,
            job_type,
            story_id,
            user_id,
            payload,
        })
    })
}

pub async fn get_job(pool: &PgPool, job_id: Uuid, user_id: Uuid) -> Result<JobRecord, String> {
    let row = sqlx::query(
        "SELECT id, job_type, story_id, user_id, status, result, error, cancel_requested,
                created_at::text, started_at::text, finished_at::text
         FROM jobs WHERE id = $1 AND user_id = $2",
    )
    .bind(job_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    match row {
        Some(r) => Ok(row_to_job_record(r)),
        None => Err("Job not found.".into()),
    }
}

pub async fn job_status(pool: &PgPool, job_id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

fn row_to_job_record(r: sqlx::postgres::PgRow) -> JobRecord {
    JobRecord {
        id: r.try_get(0).unwrap_or_default(),
        job_type: r.try_get(1).unwrap_or_default(),
        story_id: r.try_get(2).unwrap_or_default(),
        user_id: r.try_get(3).unwrap_or_default(),
        status: r.try_get(4).unwrap_or_default(),
        result: r.try_get(5).ok(),
        error: r.try_get(6).ok(),
        cancel_requested: r.try_get(7).unwrap_or(false),
        created_at: r.try_get(8).unwrap_or_default(),
        started_at: r.try_get(9).ok(),
        finished_at: r.try_get(10).ok(),
    }
}

pub async fn complete_job(pool: &PgPool, job_id: Uuid, result: Value) -> Result<(), String> {
    sqlx::query(
        "UPDATE jobs SET status = 'completed', result = $2, finished_at = now() WHERE id = $1",
    )
    .bind(job_id)
    .bind(result)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn fail_job(pool: &PgPool, job_id: Uuid, error: &str, status: &str) -> Result<(), String> {
    sqlx::query(
        "UPDATE jobs SET status = $3, error = $2, finished_at = now() WHERE id = $1",
    )
    .bind(job_id)
    .bind(error)
    .bind(status)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn request_cancel(pool: &PgPool, job_id: Uuid, user_id: Uuid) -> Result<(), String> {
    let updated = sqlx::query(
        "UPDATE jobs SET cancel_requested = true
         WHERE id = $1 AND user_id = $2 AND status IN ('pending', 'running')",
    )
    .bind(job_id)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?
    .rows_affected();

    if updated == 0 {
        return Err("Job not found or already finished.".into());
    }
    cancel_operation();
    Ok(())
}

pub async fn is_cancel_requested(pool: &PgPool, job_id: Uuid) -> bool {
    sqlx::query_scalar(
        "SELECT cancel_requested FROM jobs WHERE id = $1 AND status = 'running'",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(false)
}

pub async fn insert_event(pool: &PgPool, job_id: Uuid, channel: &str, message: &str) {
    let _ = sqlx::query(
        "INSERT INTO job_events (job_id, channel, message) VALUES ($1, $2, $3)",
    )
    .bind(job_id)
    .bind(channel)
    .bind(message)
    .execute(pool)
    .await;
}

pub async fn fetch_events_since(pool: &PgPool, job_id: Uuid, after_id: i64) -> Vec<JobEvent> {
    sqlx::query(
        "SELECT id, channel, message FROM job_events
         WHERE job_id = $1 AND id > $2 ORDER BY id LIMIT 100",
    )
    .bind(job_id)
    .bind(after_id)
    .fetch_all(pool)
    .await
    .ok()
    .map(|rows| {
        rows.into_iter()
            .filter_map(|r| {
                Some(JobEvent {
                    id: r.try_get(0).ok()?,
                    channel: r.try_get(1).ok()?,
                    message: r.try_get(2).ok()?,
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Run a claimed job — called by the worker process.
pub async fn execute_job(app: AppCtx, job: ClaimedJob) {
    let job_id = job.id;
    let pool = app.db.pool.clone();

    reset_cancel();

    let cancel_watch = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if is_cancel_requested(&pool, job_id).await {
                cancel_operation();
                break;
            }
            let status = job_status(&pool, job_id).await;
            if status.as_deref() != Some("running") {
                break;
            }
        }
    });

    let app = app.with_user_id(job.user_id).with_job_id(job_id);
    let genre_result = run_job_inner(app.clone(), &job).await;

    cancel_watch.abort();

    let pool = &app.db.pool;
    if is_cancelled() || genre_result.error == "Cancelled." {
        let _ = fail_job(pool, job_id, "Cancelled.", "cancelled").await;
        return;
    }

    if genre_result.success {
        let result = serde_json::to_value(&genre_result).unwrap_or(json!({}));
        let _ = complete_job(pool, job_id, result).await;
    } else {
        let _ = fail_job(pool, job_id, &genre_result.error, "failed").await;
    }
}

async fn run_job_inner(app: AppCtx, job: &ClaimedJob) -> GenreResult {
    match job.job_type.as_str() {
        JOB_ANALYZE_STORY => {
            let request: AnalyzeStoryRequest = match serde_json::from_value(job.payload.clone()) {
                Ok(r) => r,
                Err(e) => {
                    return GenreResult {
                        success: false,
                        report: String::new(),
                        error: format!("Invalid job payload: {e}"),
                        run_ts: String::new(),
                    };
                }
            };
            pipeline::analyze_story(app, request).await
        }
        JOB_CRAFT_PIPELINE => {
            let request: pipeline::CraftPipelineRequest =
                match serde_json::from_value(job.payload.clone()) {
                    Ok(r) => r,
                    Err(e) => {
                        return GenreResult {
                            success: false,
                            report: String::new(),
                            error: format!("Invalid job payload: {e}"),
                            run_ts: String::new(),
                        };
                    }
                };
            pipeline::run_craft_pipeline(app, request).await
        }
        JOB_MARKET_INTEL => {
            let request: MarketIntelRequest = match serde_json::from_value(job.payload.clone()) {
                Ok(r) => r,
                Err(e) => {
                    return GenreResult {
                        success: false,
                        report: String::new(),
                        error: format!("Invalid job payload: {e}"),
                        run_ts: String::new(),
                    };
                }
            };
            canopy::run_market_intel(app, request).await
        }
        other => GenreResult {
            success: false,
            report: String::new(),
            error: format!("Unknown job type: {other}"),
            run_ts: String::new(),
        },
    }
}
