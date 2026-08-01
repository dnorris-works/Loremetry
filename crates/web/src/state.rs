use std::sync::Arc;

use loremetry_core::platform_secrets::PlatformSecrets;
use loremetry_core::{AppCtx, Config};
use tokio::sync::RwLock;

use crate::auth::JwtVerifier;

#[derive(Clone)]
pub struct AppState {
    pub ctx: AppCtx,
    pub config: Arc<Config>,
    pub secrets: Arc<PlatformSecrets>,
    pub jwt: JwtVerifier,
    /// Server-auto-selected cheapest model (fetched from provider on startup).
    pub default_model: Arc<RwLock<String>>,
}
