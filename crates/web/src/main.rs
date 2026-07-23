mod admin;
mod error;
mod invoke;
mod routes;
mod sse;
mod state;
mod upload;

use std::net::SocketAddr;
use std::sync::Arc;

use loremetry_core::{db, AppCtx, Config};
use loremetry_core::config::{
    database_url_diagnostics, database_url_host, resolve_database_url_with_source,
};
use tracing_subscriber::EnvFilter;

use crate::state::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env();

    let Some((database_url, database_env)) = resolve_database_url_with_source() else {
        let diag = database_url_diagnostics();
        tracing::error!("No valid Postgres URL in environment. {diag}");
        eprintln!("FATAL: Set DATABASE_URL (or POSTGRES_*_URL from Miget) to a postgres:// connection string at runtime.");
        eprintln!("{diag}");
        std::process::exit(1);
    };

    tracing::info!(
        "Starting Loremetry (port={}, db={}, env={})",
        config.port,
        database_url_host(&database_url),
        database_env
    );

    tracing::info!("Boot: connecting to database and running migrations…");

    let database = match db::init(&database_url).await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Database init failed: {e}");
            eprintln!("Database init failed: {e}");
            if e.contains("migrations") {
                eprintln!(
                    "Hint: Check DATABASE_URL and Postgres reachability. If you see _sqlx_migrations does not exist, redeploy the latest image (migrate connection must use public search_path for sqlx)."
                );
            }
            if e.contains("previously applied but has been modified") {
                eprintln!(
                    "Hint: A migration file changed after it was applied. In SQL: SELECT * FROM public._sqlx_migrations; — contact support or fix checksum only if you know the schema is already correct."
                );
            }
            if e.contains("does not exist") && e.contains("users") {
                eprintln!("Hint: Migration 002 may not have completed. Redeploy the latest image (idempotent migration 002).");
            }
            std::process::exit(1);
        }
    };

    tracing::info!("Boot: loading platform secrets…");
    let secrets = match loremetry_core::platform_secrets::PlatformSecrets::load(
        database.pool.clone(),
    )
    .await
    {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::error!("Platform secrets init failed: {e}");
            eprintln!("Platform secrets init failed: {e}");
            if e.contains("SECRETS_ENCRYPTION_KEY") {
                eprintln!(
                    "Hint: On Miget, add SECRETS_ENCRYPTION_KEY (32 random bytes, base64). Example: openssl rand -base64 32 — then set provider API keys in Admin → Platform credentials."
                );
            } else if e.contains("decrypt failed") {
                eprintln!(
                    "Hint: SECRETS_ENCRYPTION_KEY may have changed since credentials were saved. Restore the original key or reset lore.platform_secrets ciphertext in the database."
                );
            }
            std::process::exit(1);
        }
    };

    let ctx = AppCtx::new(database);
    let state = AppState {
        ctx,
        config: Arc::new(config),
        secrets,
    };

    let port = state.config.port;
    let app = routes::build_router(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Loremetry listening on http://{addr}");

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Bind failed on {addr}: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("Server error: {e}");
        std::process::exit(1);
    }
}
