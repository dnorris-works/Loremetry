use std::sync::Arc;

use loremetry_core::platform_secrets::PlatformSecrets;
use loremetry_core::{AppCtx, Config};

use crate::auth::AuthState;

#[derive(Clone)]
pub struct AppState {
    pub ctx: AppCtx,
    pub config: Arc<Config>,
    pub secrets: Arc<PlatformSecrets>,
    pub auth: AuthState,
}
