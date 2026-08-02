mod auth;
mod admin;
mod error;
mod health;
mod invoke;
mod maintenance;
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
use loremetry_core::commands;
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

use crate::auth::JwtVerifier;
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
        eprintln!("FATAL: Set DATABASE_URL (or POSTGRES_*_URL) to a postgres:// connection string at runtime.");
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

    tracing::info!("Boot: loading platform secrets from environment variables…");
    let secrets = Arc::new(loremetry_core::platform_secrets::PlatformSecrets::load_from_env());

    let ctx = AppCtx::new(database);
    let default_model = Arc::new(RwLock::new(String::new()));
    let state = AppState {
        ctx,
        config: Arc::new(config),
        secrets,
        jwt: JwtVerifier::new(),
        default_model: default_model.clone(),
    };

    // Spawn background task to auto-select the cheapest model from the provider.
    {
        let secrets = state.secrets.clone();
        let db_handle = state.ctx.db.clone();
        let dm = default_model.clone();
        tokio::spawn(async move {
            let provider = secrets.default_provider().await;
            let api_key = secrets.resolve_api_key(&provider).await;
            if api_key.trim().is_empty() {
                tracing::warn!("No API key for provider '{provider}' — skipping auto model selection");
                return;
            }
            match commands::list_models(&db_handle, provider.clone(), api_key).await {
                Ok(result) if result.success && !result.models.is_empty() => {
                    // Sort by input_price ascending; models without pricing go to the end.
                    let mut priced: Vec<_> = result.models.iter()
                        .filter(|m| m.input_price.is_some())
                        .collect();
                    priced.sort_by(|a, b| {
                        a.input_price.unwrap().partial_cmp(&b.input_price.unwrap()).unwrap_or(std::cmp::Ordering::Equal)
                    });

                    // Prefer the cheapest "capable" model (input_price >= $0.0001/1K tokens).
                    // Ultra-cheap models often can't handle structured extraction prompts.
                    let capable: Vec<_> = priced.iter()
                        .filter(|m| m.input_price.unwrap_or(0.0) >= 0.0001)
                        .collect();
                    let selected = capable.first().copied().or(priced.first());

                    if let Some(cheapest) = selected {
                        let mut lock = dm.write().await;
                        *lock = cheapest.id.clone();
                        tracing::info!("Auto-selected default model: {} (input_price: {:?})", cheapest.id, cheapest.input_price);
                    } else {
                        tracing::warn!("No models with pricing data found — no auto-selection");
                    }
                }
                Ok(result) => {
                    tracing::warn!("Model fetch returned no usable models: {}", result.error);
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch models for auto-selection: {e}");
                }
            }
        });
    }

    if state.secrets.get().await.clerk_jwt_issuer.trim().is_empty() {
        tracing::warn!(
            "Clerk not configured — users need operator bypass or Clerk settings in Admin → Platform credentials."
        );
    } else {
        tracing::info!("Clerk sign-in enabled from platform settings in database");
    }

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
