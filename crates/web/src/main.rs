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
    if let Err(e) = std::fs::create_dir_all(&config.data_dir) {
        eprintln!("Failed to create data_dir {:?}: {e}", config.data_dir);
        std::process::exit(1);
    }

    let database = match db::init(&config.data_dir) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Database init failed: {e}");
            std::process::exit(1);
        }
    };

    let ctx = AppCtx::new(database, config.data_dir.clone());
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
