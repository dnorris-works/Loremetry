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
use loremetry_core::config::{database_url_host, resolve_database_url};
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

    let Some(database_url) = resolve_database_url() else {
        tracing::error!(
            "No Postgres URL found. Set DATABASE_URL or attach the Miget PostgreSQL addon (e.g. POSTGRES_DBWEI_URL)."
        );
        eprintln!("FATAL: No Postgres URL in environment (DATABASE_URL, POSTGRES_DBWEI_URL, …).");
        std::process::exit(1);
    };

    tracing::info!(
        "Starting Loremetry (port={}, db={})",
        config.port,
        database_url_host(&database_url)
    );

    let database = match db::init(&database_url).await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Database init failed: {e}");
            eprintln!("Database init failed: {e}");
            std::process::exit(1);
        }
    };

    let ctx = AppCtx::new(database);
    let port = config.port;
    let state = AppState {
        ctx,
        config: Arc::new(config),
    };

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
