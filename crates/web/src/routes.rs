use axum::middleware;
use axum::extract::{DefaultBodyLimit, State};
use axum::extract::{Path, Query};
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use loremetry_core::analysis::pipeline;
use loremetry_core::canopy::{self, MarketIntelRequest};
use loremetry_core::commands;
use loremetry_core::dataforseo;
use loremetry_core::db;
use loremetry_core::documents::{self, UpsertDocumentRequest};
use loremetry_core::series::{self, CreateSeriesRequest, UpdateSeriesRequest};
use loremetry_core::stories::{self, InitStoryRequest, UpdateStoryRequest};
use loremetry_core::cancel_operation;
use serde::Deserialize;
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::admin;
use crate::auth::{self, Authenticated};
use crate::error::{json_error, ok_json, result_to_response};
use crate::invoke;
use crate::sse;
use crate::state::AppState;
use crate::upload;

pub fn build_router(state: AppState) -> Router {
    let static_dir = state.config.static_dir.clone();
    let spa_index = static_dir.join("index.html");

    let admin = Router::new()
        .route("/status", get(admin::admin_status))
        .route("/tables", get(admin::admin_tables))
        .route("/sql", post(admin::admin_sql))
        .route("/winningcat/import", post(admin::winningcat_import_json))
        .route("/winningcat/upload", post(admin::winningcat_import_upload))
        .route("/winningcat/remove-stale", post(admin::winningcat_remove_stale))
        .route(
            "/platform-secrets",
            get(admin::get_platform_secrets)
                .put(admin::put_platform_secrets)
                .post(admin::put_platform_secrets),
        )
        .route("/platform-secrets/test-canopy", post(admin::test_platform_canopy))
        .route("/platform-secrets/test-dataforseo", post(admin::test_platform_dataforseo))
        .route("/test-canopy", post(admin::test_platform_canopy))
        .route("/test-dataforseo", post(admin::test_platform_dataforseo))
        .route("/usage/summary", get(admin::admin_usage_summary))
        .route("/usage/events", get(admin::admin_usage_events));

    let protected = Router::new()
        .route("/me", get(auth::auth_me))
        // Primary: generic invoke bridge
        .route("/invoke", post(invoke::invoke_handler))
        // SSE
        .route("/events", get(sse::events_handler))
        // Stories
        .route("/stories", get(list_stories).post(create_story))
        .route(
            "/stories/{id}",
            put(update_story).delete(delete_story),
        )
        // Documents
        .route(
            "/stories/{story_id}/documents",
            get(list_documents).post(upsert_document),
        )
        .route(
            "/stories/{story_id}/documents/upload",
            post(upload::upload_chapters),
        )
        .route(
            "/stories/{story_id}/files",
            get(list_manuscript_files),
        )
        .route(
            "/documents/{id}",
            get(get_document).put(update_document).delete(delete_document),
        )
        .route("/documents/{id}/fix", post(fix_document))
        // Analysis
        .route("/analysis/state", post(analysis_state))
        .route("/analysis/analyze_story", post(analyze_story))
        .route("/analysis/craft", post(craft_pipeline))
        .route("/analysis/market_intel", post(market_intel))
        .route("/analysis/cancel", post(analysis_cancel))
        .route("/activity_log", post(activity_log))
        // Reports
        .route("/reports/sidebar", post(sidebar_reports))
        .route("/reports/{id}", get(get_report).delete(delete_report))
        .route("/report-types", get(report_types))
        // Series
        .route("/series", get(list_series).post(create_series))
        .route(
            "/series/{id}",
            put(update_series).delete(delete_series),
        )
        // Settings
        .route("/models", get(list_models))
        .route("/settings/test-canopy", post(test_canopy))
        .route("/settings/test-dataforseo", post(test_dataforseo))
        .nest("/admin", admin)
        // Chat / costs / suggests
        .route("/chat", post(chat))
        .route("/costs/estimate", post(estimate_costs))
        .route("/suggest/sdt", post(suggest_sdt))
        .route("/suggest/ai-isms", post(suggest_ai_isms))
        .route("/suggest/continuity", post(suggest_continuity));

    let api = Router::new()
        .route("/auth/config", get(auth::auth_config))
        .route("/auth/session", get(auth::auth_session))
        .merge(protected)
        .with_state(state.clone());

    let static_service = ServeDir::new(&static_dir)
        .not_found_service(ServeFile::new(spa_index));

    Router::new()
        .nest("/api", api)
        .fallback_service(static_service)
        .layer(DefaultBodyLimit::max(state.config.max_body_bytes))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

// ── Stories ───────────────────────────────────────────────────────────────────

async fn list_stories(State(state): State<AppState>) -> impl IntoResponse {
    ok_json(serde_json::to_value(stories::list_stories(state.ctx).await).unwrap_or(json!(null)))
}

#[derive(Deserialize)]
struct NameBody {
    name: String,
}

async fn create_story(
    State(state): State<AppState>,
    Json(body): Json<NameBody>,
) -> impl IntoResponse {
    ok_json(
        serde_json::to_value(
            stories::init_story(state.ctx, InitStoryRequest { name: body.name }).await,
        )
        .unwrap_or(json!(null)),
    )
}

#[derive(Deserialize)]
struct UpdateStoryBody {
    name: String,
    #[serde(default)]
    bible_path: String,
}

async fn update_story(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateStoryBody>,
) -> impl IntoResponse {
    ok_json(
        serde_json::to_value(
            stories::update_story(
                state.ctx,
                UpdateStoryRequest {
                    id,
                    name: body.name,
                    bible_path: body.bible_path,
                },
            )
            .await,
        )
        .unwrap_or(json!(null)),
    )
}

async fn delete_story(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    ok_json(serde_json::to_value(stories::delete_story(state.ctx, id).await).unwrap_or(json!(null)))
}

// ── Documents ─────────────────────────────────────────────────────────────────

async fn list_documents(
    State(state): State<AppState>,
    Path(story_id): Path<String>,
) -> impl IntoResponse {
    match documents::list_documents_db(&state.ctx.db, &story_id).await {
        Ok(docs) => ok_json(json!({ "success": true, "documents": docs, "error": "" })),
        Err(e) => json_error(e),
    }
}

async fn upsert_document(
    State(state): State<AppState>,
    Path(story_id): Path<String>,
    Json(mut body): Json<UpsertDocumentRequest>,
) -> impl IntoResponse {
    body.story_id = story_id;
    match documents::upsert_document(&state.ctx.db.pool, &body).await {
        Ok(doc) => ok_json(json!({ "success": true, "document": doc, "error": "" })),
        Err(e) => json_error(e),
    }
}

async fn list_manuscript_files(
    State(state): State<AppState>,
    Path(story_id): Path<String>,
) -> impl IntoResponse {
    result_to_response(commands::list_manuscript_files(state.ctx, story_id).await)
}

async fn get_document(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match documents::get_document(&state.ctx.db.pool, id).await {
        Ok(Some(doc)) => ok_json(json!({ "success": true, "document": doc, "error": "" })),
        Ok(None) => json_error("Document not found"),
        Err(e) => json_error(e),
    }
}

async fn update_document(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Some(content) = body.get("content").and_then(|v| v.as_str()) {
        let doc = match documents::get_document(&state.ctx.db.pool, id).await {
            Ok(Some(d)) => d,
            Ok(None) => return json_error("Document not found"),
            Err(e) => return json_error(e),
        };
        let req = UpsertDocumentRequest {
            story_id: doc.story_id,
            kind: doc.kind,
            title: doc.title,
            path_hint: doc.path_hint,
            content: content.to_string(),
            id: Some(id),
        };
        return match documents::upsert_document(&state.ctx.db.pool, &req).await {
            Ok(doc) => ok_json(json!({ "success": true, "document": doc, "error": "" })),
            Err(e) => json_error(e),
        };
    }
    match serde_json::from_value::<UpsertDocumentRequest>(body) {
        Ok(mut req) => {
            req.id = Some(id);
            match documents::upsert_document(&state.ctx.db.pool, &req).await {
                Ok(doc) => ok_json(json!({ "success": true, "document": doc, "error": "" })),
                Err(e) => json_error(e),
            }
        }
        Err(e) => json_error(format!("Invalid body: {e}")),
    }
}

async fn delete_document(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<StoryIdQuery>,
) -> impl IntoResponse {
    let story_id = match q.story_id {
        Some(s) => s,
        None => {
            match documents::get_document(&state.ctx.db.pool, id).await {
                Ok(Some(d)) => d.story_id,
                Ok(None) => return json_error("Document not found"),
                Err(e) => return json_error(e),
            }
        }
    };
    match documents::delete_document(&state.ctx.db.pool, &story_id, id).await {
        Ok(()) => ok_json(json!({ "success": true })),
        Err(e) => json_error(e),
    }
}

#[derive(Deserialize)]
struct StoryIdQuery {
    story_id: Option<String>,
}

#[derive(Deserialize)]
struct FixBody {
    old_text: String,
    new_text: String,
}

async fn fix_document(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<FixBody>,
) -> impl IntoResponse {
    result_to_response(
        commands::write_manuscript_fix(state.ctx, id, body.old_text, body.new_text).await,
    )
}

// ── Analysis ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct StoryIdBody {
    #[serde(alias = "folder")]
    story_id: String,
}

async fn analysis_state(
    State(state): State<AppState>,
    Json(body): Json<StoryIdBody>,
) -> impl IntoResponse {
    ok_json(
        serde_json::to_value(pipeline::check_analysis_state(state.ctx, body.story_id).await)
            .unwrap_or(json!(null)),
    )
}

async fn analyze_story(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> impl IntoResponse {
    invoke::inject_platform_credentials(&state, &mut body).await;
    match serde_json::from_value::<loremetry_core::analysis::AnalyzeStoryRequest>(body) {
        Ok(req) => ok_json(
            serde_json::to_value(pipeline::analyze_story(state.ctx, req).await).unwrap_or(json!(null)),
        ),
        Err(e) => json_error(format!("Invalid request: {e}")),
    }
}

async fn craft_pipeline(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> impl IntoResponse {
    invoke::inject_platform_credentials(&state, &mut body).await;
    match serde_json::from_value::<pipeline::CraftPipelineRequest>(body) {
        Ok(req) => ok_json(
            serde_json::to_value(pipeline::run_craft_pipeline(state.ctx, req).await)
                .unwrap_or(json!(null)),
        ),
        Err(e) => json_error(format!("Invalid request: {e}")),
    }
}

async fn market_intel(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> impl IntoResponse {
    invoke::inject_platform_credentials(&state, &mut body).await;
    match serde_json::from_value::<MarketIntelRequest>(body) {
        Ok(req) => ok_json(
            serde_json::to_value(canopy::run_market_intel(state.ctx, req).await)
                .unwrap_or(json!(null)),
        ),
        Err(e) => json_error(format!("Invalid request: {e}")),
    }
}

async fn analysis_cancel() -> impl IntoResponse {
    cancel_operation();
    ok_json(json!({ "success": true }))
}

#[derive(Deserialize)]
struct ActivityLogBody {
    #[serde(alias = "folder")]
    story_id: String,
    content: String,
    #[serde(default)]
    timestamp: String,
}

async fn activity_log(
    State(state): State<AppState>,
    Json(body): Json<ActivityLogBody>,
) -> impl IntoResponse {
    result_to_response(
        db::save_activity_log_cmd(&state.ctx.db, body.story_id, body.content, body.timestamp)
            .await,
    )
}

// ── Reports ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SidebarBody {
    #[serde(alias = "folder")]
    story_id: String,
    #[serde(default = "default_platform")]
    platform: String,
}

fn default_platform() -> String {
    "kdp".into()
}

async fn sidebar_reports(
    State(state): State<AppState>,
    Json(body): Json<SidebarBody>,
) -> impl IntoResponse {
    result_to_response(db::get_sidebar_reports(&state.ctx.db, body.story_id, body.platform).await)
}

async fn get_report(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    result_to_response(db::get_report_cmd(&state.ctx.db, id).await)
}

async fn delete_report(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    result_to_response(db::delete_report_cmd(&state.ctx.db, id).await.map(|_| json!(null)))
}

async fn report_types(State(state): State<AppState>) -> impl IntoResponse {
    result_to_response(db::list_report_types_cmd(&state.ctx.db).await)
}

// ── Series ────────────────────────────────────────────────────────────────────

async fn list_series(State(state): State<AppState>) -> impl IntoResponse {
    ok_json(serde_json::to_value(series::list_series(state.ctx).await).unwrap_or(json!(null)))
}

async fn create_series(
    State(state): State<AppState>,
    Json(body): Json<CreateSeriesRequest>,
) -> impl IntoResponse {
    ok_json(
        serde_json::to_value(series::create_series(state.ctx, body).await).unwrap_or(json!(null)),
    )
}

async fn update_series(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(mut body): Json<UpdateSeriesRequest>,
) -> impl IntoResponse {
    body.id = id;
    ok_json(
        serde_json::to_value(series::update_series(state.ctx, body).await).unwrap_or(json!(null)),
    )
}

async fn delete_series(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    ok_json(serde_json::to_value(series::delete_series(state.ctx, id).await).unwrap_or(json!(null)))
}

// ── Settings ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ModelsQuery {
    provider: Option<String>,
}

async fn list_models(
    State(state): State<AppState>,
    Query(q): Query<ModelsQuery>,
) -> impl IntoResponse {
    let provider = if let Some(p) = q.provider {
        p
    } else {
        state.secrets.default_provider().await
    };
    let api_key = state.secrets.resolve_api_key(&provider).await;
    match commands::list_models(&state.ctx.db, provider, api_key).await {
        Ok(r) => ok_json(serde_json::to_value(r).unwrap_or(json!(null))),
        Err(e) => json_error(e),
    }
}

async fn test_canopy(State(state): State<AppState>) -> impl IntoResponse {
    let key = state.secrets.canopy_key().await;
    ok_json(
        serde_json::to_value(canopy::test_canopy_connection(key).await).unwrap_or(json!(null)),
    )
}

async fn test_dataforseo(State(state): State<AppState>) -> impl IntoResponse {
    let (login, password) = state.secrets.dataforseo().await;
    ok_json(
        serde_json::to_value(dataforseo::test_dataforseo_connection(login, password).await)
            .unwrap_or(json!(null)),
    )
}

// ── Chat / costs / suggests ───────────────────────────────────────────────────

async fn chat(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> impl IntoResponse {
    invoke::inject_platform_credentials(&state, &mut body).await;
    match serde_json::from_value::<commands::ChatRequest>(body) {
        Ok(req) => match commands::chat_with_context(&state.ctx, req).await {
            Ok(r) => ok_json(serde_json::to_value(r).unwrap_or(json!(null))),
            Err(_) => json_error("Chat failed"),
        },
        Err(e) => json_error(format!("Invalid request: {e}")),
    }
}

async fn estimate_costs(
    State(state): State<AppState>,
    Json(body): Json<commands::CostEstimateRequest>,
) -> impl IntoResponse {
    result_to_response(commands::estimate_report_costs(state.ctx, body).await)
}

async fn suggest_sdt(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> impl IntoResponse {
    invoke::inject_platform_credentials(&state, &mut body).await;
    match serde_json::from_value::<
        loremetry_core::analysis::show_dont_tell::SuggestSdtFixRequest,
    >(body)
    {
        Ok(req) => ok_json(
            serde_json::to_value(
                loremetry_core::analysis::show_dont_tell::suggest_sdt_fix(state.ctx, req)
                    .await,
            )
            .unwrap_or(json!(null)),
        ),
        Err(e) => json_error(format!("Invalid request: {e}")),
    }
}

async fn suggest_ai_isms(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> impl IntoResponse {
    invoke::inject_platform_credentials(&state, &mut body).await;
    match serde_json::from_value::<
        loremetry_core::analysis::ai_isms::SuggestAiIsmsFixRequest,
    >(body)
    {
        Ok(req) => ok_json(
            serde_json::to_value(
                loremetry_core::analysis::ai_isms::suggest_ai_isms_fix(state.ctx, req)
                    .await,
            )
            .unwrap_or(json!(null)),
        ),
        Err(e) => json_error(format!("Invalid request: {e}")),
    }
}

async fn suggest_continuity(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> impl IntoResponse {
    invoke::inject_platform_credentials(&state, &mut body).await;
    match serde_json::from_value::<
        loremetry_core::analysis::continuity::SuggestFixRequest,
    >(body)
    {
        Ok(req) => ok_json(
            serde_json::to_value(
                loremetry_core::analysis::continuity::suggest_continuity_fix(
                    state.ctx, req,
                )
                .await,
            )
            .unwrap_or(json!(null)),
        ),
        Err(e) => json_error(format!("Invalid request: {e}")),
    }
}
