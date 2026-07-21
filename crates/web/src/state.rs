use std::sync::Arc;

use manuscript_intel_core::{AppCtx, Config};

#[derive(Clone)]
pub struct AppState {
    pub ctx: AppCtx,
    pub config: Arc<Config>,
}
