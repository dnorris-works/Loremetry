//! Background worker — polls `lore.jobs` and runs analysis pipelines.

use std::time::Duration;

use loremetry_core::config::{
    database_url_diagnostics, database_url_host, resolve_database_url_with_source,
};
use loremetry_core::{db, jobs, AppCtx};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let Some((database_url, database_env)) = resolve_database_url_with_source() else {
        let diag = database_url_diagnostics();
        tracing::error!("No valid Postgres URL in environment. {diag}");
        eprintln!("FATAL: Set DATABASE_URL to a postgres:// connection string.");
        std::process::exit(1);
    };

    tracing::info!(
        "Starting Loremetry worker (db={}, env={})",
        database_url_host(&database_url),
        database_env
    );

    let database = match db::init(&database_url).await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Database init failed: {e}");
            std::process::exit(1);
        }
    };

    let pool = database.pool.clone();
    let app = AppCtx::new(database);

    tracing::info!("Worker ready — polling for jobs");

    loop {
        if let Some(job) = jobs::claim_next(&pool).await {
            tracing::info!(
                "Running job {} ({}, story={})",
                job.id,
                job.job_type,
                job.story_id
            );
            jobs::execute_job(app.clone(), job).await;
            tracing::info!("Job finished");
        } else {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
