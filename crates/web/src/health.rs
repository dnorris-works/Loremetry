use axum::response::IntoResponse;
use serde::Serialize;
use serde_json::json;
use std::time::Duration;
use tokio::time::timeout;

use crate::auth::Authenticated;
use crate::error::ok_json;
use crate::state::AppState;

const PER_SERVICE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Serialize)]
pub struct ServiceStatus {
    pub configured: bool,
    pub connected: bool,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct ServiceHealthResponse {
    pub ai: ServiceStatus,
    pub canopy: ServiceStatus,
    pub dataforseo: ServiceStatus,
}

pub async fn service_health(auth: Authenticated) -> impl IntoResponse {
    let state = &auth.state;
    let status = state.secrets.configured_status().await;

    let ai_configured = status.tokenmix || status.anthropic;

    let (ai_result, canopy_result, dfs_result) = tokio::join!(
        check_ai(state, ai_configured),
        check_canopy(state, status.canopy),
        check_dataforseo(state, status.dataforseo),
    );

    ok_json(
        serde_json::to_value(ServiceHealthResponse {
            ai: ai_result,
            canopy: canopy_result,
            dataforseo: dfs_result,
        })
        .unwrap_or(json!(null)),
    )
}

async fn check_ai(state: &AppState, configured: bool) -> ServiceStatus {
    if !configured {
        return ServiceStatus {
            configured: false,
            connected: false,
            error: None,
        };
    }
    let provider = state.secrets.default_provider().await;
    let api_key = state.secrets.resolve_api_key(&provider).await;
    match timeout(
        PER_SERVICE_TIMEOUT,
        loremetry_core::commands::list_models(&state.ctx.db, provider, api_key),
    )
    .await
    {
        Ok(Ok(result)) => {
            if result.success {
                ServiceStatus {
                    configured: true,
                    connected: true,
                    error: None,
                }
            } else {
                ServiceStatus {
                    configured: true,
                    connected: false,
                    error: Some(result.error),
                }
            }
        }
        Ok(Err(e)) => ServiceStatus {
            configured: true,
            connected: false,
            error: Some(e),
        },
        Err(_) => ServiceStatus {
            configured: true,
            connected: false,
            error: Some("Connection timed out".into()),
        },
    }
}

async fn check_canopy(state: &AppState, configured: bool) -> ServiceStatus {
    if !configured {
        return ServiceStatus {
            configured: false,
            connected: false,
            error: None,
        };
    }
    let key = state.secrets.canopy_key().await;
    match timeout(
        PER_SERVICE_TIMEOUT,
        loremetry_core::canopy::test_canopy_connection(key),
    )
    .await
    {
        Ok(result) => {
            if result.success {
                ServiceStatus {
                    configured: true,
                    connected: true,
                    error: None,
                }
            } else {
                ServiceStatus {
                    configured: true,
                    connected: false,
                    error: Some(result.error),
                }
            }
        }
        Err(_) => ServiceStatus {
            configured: true,
            connected: false,
            error: Some("Connection timed out".into()),
        },
    }
}

async fn check_dataforseo(state: &AppState, configured: bool) -> ServiceStatus {
    if !configured {
        return ServiceStatus {
            configured: false,
            connected: false,
            error: None,
        };
    }
    let (login, password) = state.secrets.dataforseo().await;
    match timeout(
        PER_SERVICE_TIMEOUT,
        loremetry_core::dataforseo::test_dataforseo_connection(login, password),
    )
    .await
    {
        Ok(result) => {
            if result.success {
                ServiceStatus {
                    configured: true,
                    connected: true,
                    error: None,
                }
            } else {
                ServiceStatus {
                    configured: true,
                    connected: false,
                    error: Some(result.error),
                }
            }
        }
        Err(_) => ServiceStatus {
            configured: true,
            connected: false,
            error: Some("Connection timed out".into()),
        },
    }
}
