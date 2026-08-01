//! Generic Tauri-compatible invoke bridge: `POST /api/invoke` with `{ cmd, args }`.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use crate::auth::Authenticated;
use loremetry_core::analysis::{
    ai_isms, continuity, pipeline, readability, show_dont_tell, zeigarnik, AnalyzeStoryRequest, FolderRequest,
};
use loremetry_core::canopy::{self, MarketIntelRequest};
use loremetry_core::commands::{self, ChatRequest, CostEstimateRequest};
use loremetry_core::dataforseo;
use loremetry_core::db;
use loremetry_core::documents::{self, UpsertDocumentRequest};
use loremetry_core::series::{self, CreateSeriesRequest, UpdateSeriesRequest};
use loremetry_core::stories::{self, InitStoryRequest, UpdateStoryRequest};
use loremetry_core::cancel_operation;
use loremetry_core::jobs;
use loremetry_core::campaigns;
use loremetry_core::platform_secrets::PlatformCredentialsPatch;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{json_error, ok_json};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct InvokeBody {
    pub cmd: String,
    #[serde(default)]
    pub args: Value,
}

pub async fn invoke_handler(
    auth: Authenticated,
    Json(body): Json<InvokeBody>,
) -> impl IntoResponse {
    if crate::auth::invoke_requires_operator(&body.cmd) && !auth.user.is_admin() {
        return json_error("Operator access required");
    }
    let state = &auth.state;
    let ctx = auth.ctx();
    let mut args = normalize_args(body.args);
    // Connection tests may pass unsaved form values; do not strip or replace them.
    let skip_credential_inject = matches!(
        body.cmd.as_str(),
        "test_canopy_connection"
            | "test_dataforseo_connection"
            | "get_platform_credentials"
            | "update_platform_credentials"
    );
    if !skip_credential_inject {
        inject_platform_credentials(&state, &mut args).await;
    }
    match dispatch(state, &ctx, &body.cmd, args).await {
        Ok(v) => ok_json(v),
        Err(e) => json_error(e),
    }
}

async fn dispatch(state: &AppState, app: &loremetry_core::AppCtx, cmd: &str, mut args: Value) -> Result<Value, String> {
    let app = app.clone();
    let db = app.db.clone();

    match cmd {
        // ── Stories ──────────────────────────────────────────────────────────
        "list_stories" => to_val(stories::list_stories(app).await),
        "init_story" | "add_story" => {
            let request: InitStoryRequest = take_request(&args)?;
            to_val(stories::init_story(app, request).await)
        }
        "update_story" => {
            let request: UpdateStoryRequest = take_request(&args)?;
            to_val(stories::update_story(app, request).await)
        }
        "delete_story" => {
            let id = take_string(&args, &["id"])?;
            to_val(stories::delete_story(app, id).await)
        }
        "create_story_document" => create_story_document(state, &args).await,
        "delete_story_document" => delete_story_document(state, &args).await,

        // ── Documents / manuscript ───────────────────────────────────────────
        "list_manuscript_files" => {
            let story_id = take_story_id(&args)?;
            to_val(commands::list_manuscript_files(app, story_id).await?)
        }
        "read_chapter" => {
            let doc_id = take_doc_id(&args, &["file_path", "doc_id", "id"])?;
            to_val(commands::read_chapter(app, doc_id).await?)
        }
        "save_chapter" => {
            let doc_id = take_doc_id(&args, &["file_path", "doc_id", "id"])?;
            let content = take_string(&args, &["content"])?;
            commands::save_chapter(app, doc_id, content).await?;
            Ok(json!(null))
        }
        "write_manuscript_fix" => {
            let doc_id = take_doc_id(&args, &["file_path", "doc_id", "id"])?;
            let old_text = take_string(&args, &["old_text"])?;
            let new_text = take_string(&args, &["new_text"])?;
            to_val(commands::write_manuscript_fix(app, doc_id, old_text, new_text).await?)
        }

        // ── Series ───────────────────────────────────────────────────────────
        "list_series" => to_val(series::list_series(app).await),
        "create_series" => {
            let request: CreateSeriesRequest = take_request(&args)?;
            to_val(series::create_series(app, request).await)
        }
        "update_series" => {
            let request: UpdateSeriesRequest = take_request(&args)?;
            to_val(series::update_series(app, request).await)
        }
        "delete_series" => {
            let id = take_i64(&args, &["id"])?;
            to_val(series::delete_series(app, id).await)
        }

        // ── Analysis ─────────────────────────────────────────────────────────
        "check_analysis_state" => {
            let story_id = take_story_id(&args)?;
            to_val(pipeline::check_analysis_state(app, story_id).await)
        }
        "analyze_story" => {
            let request: AnalyzeStoryRequest = take_request(&args)?;
            to_val(
                jobs::enqueue(
                    &db.pool,
                    jobs::JOB_ANALYZE_STORY,
                    &request.story_id,
                    app.user_id(),
                    serde_json::to_value(&request).unwrap_or(json!({})),
                )
                .await?,
            )
        }
        "run_craft_pipeline" => {
            let request: pipeline::CraftPipelineRequest = take_request(&args)?;
            to_val(
                jobs::enqueue(
                    &db.pool,
                    jobs::JOB_CRAFT_PIPELINE,
                    &request.story_id,
                    app.user_id(),
                    serde_json::to_value(&request).unwrap_or(json!({})),
                )
                .await?,
            )
        }
        "save_zeigarnik_report" => {
            let request: zeigarnik::SaveZeigarnikClientRequest = take_request(&args)?;
            to_val(zeigarnik::save_zeigarnik_client_report(app, request).await)
        }
        "save_readability_report" => {
            let request: readability::SaveReadabilityClientRequest = take_request(&args)?;
            to_val(readability::save_readability_client_report(app, request).await)
        }
        "run_market_intel" => {
            let request: MarketIntelRequest = take_request(&args)?;
            to_val(
                jobs::enqueue(
                    &db.pool,
                    jobs::JOB_MARKET_INTEL,
                    &request.story_id,
                    app.user_id(),
                    serde_json::to_value(&request).unwrap_or(json!({})),
                )
                .await?,
            )
        }
        "run_everything" => {
            let request: FolderRequest = take_request(&args)?;
            to_val(pipeline::run_everything(app, request).await)
        }
        "run_full_analysis" => {
            let request: FolderRequest = take_request(&args)?;
            to_val(pipeline::run_full_analysis(app, request).await)
        }
        "cancel_operation" => {
            if let Some(job_id) = optional_string(&args, &["job_id", "jobId"]) {
                if let Ok(id) = uuid::Uuid::parse_str(&job_id) {
                    let _ = jobs::request_cancel(&db.pool, id, app.user_id()).await;
                }
            } else {
                cancel_operation();
            }
            Ok(json!(null))
        }
        "save_activity_log_cmd" => {
            let folder = take_story_id(&args)?;
            let content = take_string(&args, &["content"])?;
            let timestamp = take_string(&args, &["timestamp"]).unwrap_or_default();
            db::save_activity_log_cmd(&db, folder, content, timestamp).await?;
            Ok(json!(null))
        }

        // ── Suggest fixes ────────────────────────────────────────────────────
        "suggest_sdt_fix" => {
            let request: show_dont_tell::SuggestSdtFixRequest = take_request(&args)?;
            to_val(show_dont_tell::suggest_sdt_fix(app, request).await)
        }
        "suggest_ai_isms_fix" => {
            let request: ai_isms::SuggestAiIsmsFixRequest = take_request(&args)?;
            to_val(ai_isms::suggest_ai_isms_fix(app, request).await)
        }
        "suggest_continuity_fix" => {
            let request: continuity::SuggestFixRequest = take_request(&args)?;
            to_val(continuity::suggest_continuity_fix(app, request).await)
        }

        // ── Reports ──────────────────────────────────────────────────────────
        "get_sidebar_reports" => {
            let folder = take_story_id(&args)?;
            let platform = take_string(&args, &["platform"]).unwrap_or_else(|_| "kdp".into());
            to_val(db::get_sidebar_reports(&db, folder, platform).await?)
        }
        "get_report_cmd" => {
            let id = take_i64(&args, &["id"])?;
            to_val(db::get_report_cmd(&db, id).await?)
        }
        "delete_report_cmd" => {
            let id = take_i64(&args, &["id"])?;
            db::delete_report_cmd(&db, id).await?;
            Ok(json!(null))
        }
        "list_report_types_cmd" => to_val(db::list_report_types_cmd(&db).await?),
        "list_craft_report_groups_cmd" => {
            to_val(loremetry_core::craft_report_groups::list_craft_report_groups_cmd().await?)
        }
        "list_reports_cmd" => {
            let folder = take_story_id(&args)?;
            to_val(db::list_reports_cmd(&db, folder).await?)
        }
        "get_story_artifact_state" => {
            let folder = take_story_id(&args)?;
            to_val(loremetry_core::story_settings::get_story_artifact_state(app, folder).await?)
        }
        "refresh_chapter_summaries" => {
            let request: FolderRequest = take_request(&args)?;
            to_val(loremetry_core::story_settings::refresh_chapter_summaries(app, request).await?)
        }
        "get_ai_spend_totals" => {
            to_val(commands::get_ai_spend_totals(&app).await?)
        }
        "clear_chapter_summaries" => {
            let folder = take_story_id(&args)?;
            loremetry_core::story_settings::clear_chapter_summaries(app, folder).await?;
            Ok(json!(null))
        }
        "get_archived_reports" => {
            let folder = take_story_id(&args)?;
            to_val(loremetry_core::story_settings::get_archived_reports(app, folder).await?)
        }

        // ── Settings / external APIs ─────────────────────────────────────────
        "list_models" => {
            let provider = take_string(&args, &["provider"])?;
            let api_key = state.secrets.resolve_api_key(&provider).await;
            to_val(commands::list_models(&db, provider, api_key).await?)
        }
        "test_canopy_connection" => {
            let key = match optional_string(&args, &["canopy_api_key", "canopyApiKey"])
                .filter(|s| !s.trim().is_empty())
            {
                Some(k) => k,
                None => state.secrets.canopy_key().await,
            };
            to_val(canopy::test_canopy_connection(key).await)
        }
        "test_dataforseo_connection" => {
            let login = optional_string(&args, &["dataforseo_login", "dataforseoLogin"]);
            let password = optional_string(&args, &["dataforseo_password", "dataforseoPassword"]);
            let (login, password) = match (login, password) {
                (Some(l), Some(p)) if !l.trim().is_empty() && !p.trim().is_empty() => (l, p),
                _ => state.secrets.dataforseo().await,
            };
            to_val(dataforseo::test_dataforseo_connection(login, password).await)
        }
        "get_platform_credentials" => to_val(state.secrets.admin_get().await),
        "update_platform_credentials" => {
            let patch: PlatformCredentialsPatch = take_request(&args)?;
            state.secrets.update(patch).await?;
            state.jwt.reset_cache().await;
            let credentials = state.secrets.admin_get().await;
            Ok(json!({ "success": true, "credentials": credentials }))
        }
        "import_winningcat_csv" | "remove_stale_kdp_categories" => {
            Err("WinningCat import is admin-only. Use /api/admin/winningcat or the Admin panel.".into())
        }
        "get_folder_structure" => Ok(default_folder_structure()),
        "save_folder_structure" => {
            // Web mode: accept and echo structure (no filesystem scaffolding).
            if let Some(req) = args.get("structure").or_else(|| args.get("request")) {
                Ok(req.clone())
            } else {
                Ok(default_folder_structure())
            }
        }
        "pick_manuscript_folder" => {
            Err("pick_manuscript_folder is not available in web mode; upload documents instead.".into())
        }

        // ── Chat / costs ─────────────────────────────────────────────────────
        "chat_with_context" => {
            let request: ChatRequest = take_request(&args)?;
            match commands::chat_with_context(&app, request).await {
                Ok(r) => to_val(r),
                Err(_) => Err("Chat failed.".into()),
            }
        }
        "estimate_report_costs" => {
            let request: CostEstimateRequest = take_request(&args)?;
            to_val(commands::estimate_report_costs(app, request).await?)
        }
        "estimate_summary_refresh_cost" => {
            let request: commands::SummaryRefreshEstimateRequest = take_request(&args)?;
            to_val(commands::estimate_summary_refresh_cost(app, request).await?)
        }

        // ── Misc analysis commands ───────────────────────────────────────────
        "analyze_csv" => {
            let request: commands::CsvRequest = take_request(&args)?;
            to_val(commands::analyze_csv(app, request).await)
        }
        "get_genre_taxonomy" => to_val(loremetry_core::genre_taxonomy::get_genre_taxonomy(&db).await?),
        "list_genres_cmd" => to_val(db::list_genres_cmd(&db).await?),

        // ── Marketing / campaigns ────────────────────────────────────────────
        "list_campaigns" => {
            let story_id = take_story_id(&args)?;
            to_val(campaigns::list_campaigns(app, story_id).await)
        }
        "create_campaign" => {
            let request: campaigns::CreateCampaignRequest = take_request(&args)?;
            to_val(campaigns::create_campaign(app, request).await)
        }
        "update_campaign" => {
            let request: campaigns::UpdateCampaignRequest = take_request(&args)?;
            to_val(campaigns::update_campaign(app, request).await)
        }
        "delete_campaign" => {
            let id = take_i64(&args, &["id"])?;
            to_val(campaigns::delete_campaign(app, id).await)
        }
        "get_campaign_detail" => {
            let id = take_i64(&args, &["id"])?;
            to_val(campaigns::get_campaign_detail(app, id).await)
        }
        "list_creatives" => {
            let id = take_i64(&args, &["campaign_id", "campaignId"])?;
            to_val(campaigns::list_creatives(app, id).await)
        }
        "create_creative" => {
            let request: campaigns::CreativeInput = take_request(&args)?;
            to_val(campaigns::create_creative(app, request).await)
        }
        "update_creative" => {
            let request: campaigns::UpdateCreativeRequest = take_request(&args)?;
            to_val(campaigns::update_creative(app, request).await)
        }
        "delete_creative" => {
            let id = take_i64(&args, &["id"])?;
            to_val(campaigns::delete_creative(app, id).await)
        }
        "list_performance_snapshots" => {
            let id = take_i64(&args, &["campaign_id", "campaignId"])?;
            to_val(campaigns::list_performance_snapshots(app, id).await)
        }
        "add_performance_snapshot" => {
            let request: campaigns::SnapshotInput = take_request(&args)?;
            to_val(campaigns::add_performance_snapshot(app, request).await)
        }
        "delete_performance_snapshot" => {
            let id = take_i64(&args, &["id"])?;
            to_val(campaigns::delete_performance_snapshot(app, id).await)
        }
        "list_spend_entries" => {
            let id = take_i64(&args, &["campaign_id", "campaignId"])?;
            to_val(campaigns::list_spend_entries(app, id).await)
        }
        "add_spend_entry" => {
            let request: campaigns::SpendInput = take_request(&args)?;
            to_val(campaigns::add_spend_entry(app, request).await)
        }
        "delete_spend_entry" => {
            let id = take_i64(&args, &["id"])?;
            to_val(campaigns::delete_spend_entry(app, id).await)
        }
        "list_landing_pages" => {
            let story_id = take_story_id(&args)?;
            to_val(campaigns::list_landing_pages(app, story_id).await)
        }
        "create_landing_page" => {
            let request: campaigns::LandingPageInput = take_request(&args)?;
            to_val(campaigns::create_landing_page(app, request).await)
        }
        "update_landing_page" => {
            let request: campaigns::UpdateLandingPageRequest = take_request(&args)?;
            to_val(campaigns::update_landing_page(app, request).await)
        }
        "delete_landing_page" => {
            let id = take_i64(&args, &["id"])?;
            to_val(campaigns::delete_landing_page(app, id).await)
        }
        "list_audience_notes" => {
            let id = take_i64(&args, &["campaign_id", "campaignId"])?;
            to_val(campaigns::list_audience_notes(app, id).await)
        }
        "add_audience_note" => {
            let request: campaigns::AudienceNoteInput = take_request(&args)?;
            to_val(campaigns::add_audience_note(app, request).await)
        }
        "update_audience_note" => {
            let request: campaigns::UpdateAudienceNoteRequest = take_request(&args)?;
            to_val(campaigns::update_audience_note(app, request).await)
        }
        "delete_audience_note" => {
            let id = take_i64(&args, &["id"])?;
            to_val(campaigns::delete_audience_note(app, id).await)
        }
        "list_platform_accounts" => to_val(campaigns::list_platform_accounts(app).await),
        "create_platform_account" => {
            let request: campaigns::PlatformAccountInput = take_request(&args)?;
            to_val(campaigns::create_platform_account(app, request).await)
        }
        "update_platform_account" => {
            let request: campaigns::UpdatePlatformAccountRequest = take_request(&args)?;
            to_val(campaigns::update_platform_account(app, request).await)
        }
        "delete_platform_account" => {
            let id = take_i64(&args, &["id"])?;
            to_val(campaigns::delete_platform_account(app, id).await)
        }

        other => Err(format!("Unknown command: {other}")),
    }
}

// ── Document helpers (replacing filesystem create/delete) ────────────────────

async fn create_story_document(state: &AppState, args: &Value) -> Result<Value, String> {
    let req_val = args.get("request").unwrap_or(args);
    let story_id = take_string(req_val, &["story_folder", "story_id", "folder"])?;
    let name = take_string(req_val, &["name"])?;
    let location = take_string(req_val, &["location"]).unwrap_or_default();

    if !stories::story_exists(&state.ctx.db, &story_id).await {
        return Err(format!("Story not found: {story_id}"));
    }
    let title = name.trim().to_string();
    if title.is_empty() {
        return Err("Please enter a document name.".into());
    }

    let kind = kind_from_location(&location);
    let path_hint = if location.is_empty() {
        format!("{title}.md")
    } else {
        format!(
            "{}/{}",
            location.trim_matches('/'),
            if title.ends_with(".md") {
                title.clone()
            } else {
                format!("{title}.md")
            }
        )
    };

    let upsert = UpsertDocumentRequest {
        story_id,
        kind,
        title: title.clone(),
        path_hint,
        content: format!("# {title}\n\n"),
        id: None,
    };

    let doc = documents::upsert_document(&state.ctx.db.pool, &upsert).await?;
    Ok(json!({
        "path": format!("doc:{}", doc.id),
        "title": doc.title,
    }))
}

async fn delete_story_document(state: &AppState, args: &Value) -> Result<Value, String> {
    let story_id = take_string(args, &["story_folder", "story_id", "folder"])?;
    let doc_id = take_doc_id(args, &["file_path", "doc_id", "id", "path"])?;
    documents::delete_document(&state.ctx.db.pool, &story_id, doc_id).await?;
    Ok(json!(null))
}

fn kind_from_location(location: &str) -> String {
    let lower = location.to_lowercase();
    if lower.contains("bible") {
        "bible".into()
    } else if lower.contains("character") {
        "character".into()
    } else if lower.contains("location") {
        "location".into()
    } else {
        "chapter".into()
    }
}

fn default_folder_structure() -> Value {
    json!({
        "manuscript": "Manuscript",
        "bible": "Bible",
        "characters": "Characters",
        "locations": "Locations",
        "acts": ["Act-1", "Act-2", "Act-3"],
        "extra": ["Publishing/Cover", "Research"],
    })
}

// ── Arg helpers ──────────────────────────────────────────────────────────────

fn to_val<T: serde::Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

fn take_request<T: serde::de::DeserializeOwned>(args: &Value) -> Result<T, String> {
    let src = args.get("request").unwrap_or(args);
    serde_json::from_value(src.clone()).map_err(|e| format!("Invalid request: {e}"))
}

fn optional_string(args: &Value, keys: &[&str]) -> Option<String> {
    take_string(args, keys).ok()
}

fn take_string(args: &Value, keys: &[&str]) -> Result<String, String> {
    for k in keys {
        if let Some(v) = args.get(*k) {
            if let Some(s) = v.as_str() {
                return Ok(s.to_string());
            }
            if v.is_number() {
                return Ok(v.to_string());
            }
        }
    }
    // Also check inside request
    if let Some(req) = args.get("request") {
        for k in keys {
            if let Some(v) = req.get(*k) {
                if let Some(s) = v.as_str() {
                    return Ok(s.to_string());
                }
            }
        }
    }
    Err(format!("Missing string field (tried: {})", keys.join(", ")))
}

fn take_i64(args: &Value, keys: &[&str]) -> Result<i64, String> {
    for k in keys {
        if let Some(v) = args.get(*k) {
            if let Some(n) = v.as_i64() {
                return Ok(n);
            }
            if let Some(s) = v.as_str() {
                if let Ok(n) = s.parse() {
                    return Ok(n);
                }
            }
        }
    }
    Err(format!("Missing i64 field (tried: {})", keys.join(", ")))
}

fn take_story_id(args: &Value) -> Result<String, String> {
    take_string(args, &["story_id", "folder", "story_folder", "storyFolder", "id"]).or_else(|_| {
        if let Some(req) = args.get("request") {
            take_string(req, &["story_id", "folder", "story_folder", "storyFolder", "id"])
        } else {
            Err("Missing story_id/folder".into())
        }
    })
}

fn take_doc_id(args: &Value, keys: &[&str]) -> Result<i64, String> {
    for k in keys {
        if let Some(v) = args.get(*k) {
            if let Some(n) = v.as_i64() {
                return Ok(n);
            }
            if let Some(s) = v.as_str() {
                return parse_doc_ref(s);
            }
        }
    }
    if let Some(req) = args.get("request") {
        return take_doc_id(req, keys);
    }
    Err(format!("Missing document id (tried: {})", keys.join(", ")))
}

fn parse_doc_ref(s: &str) -> Result<i64, String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("doc:") {
        return rest
            .parse()
            .map_err(|_| format!("Invalid doc id: {s}"));
    }
    s.parse()
        .map_err(|_| format!("Invalid document reference: {s}"))
}

/// Convert camelCase object keys to snake_case (Tauri IPC style).
fn normalize_args(v: Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, val) in map {
                out.insert(camel_to_snake(&k), normalize_args(val));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(normalize_args).collect()),
        other => other,
    }
}

fn camel_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Inject platform credentials from server storage; ignore any client-supplied secrets.
pub async fn inject_platform_credentials(state: &AppState, args: &mut Value) {
    strip_client_secrets(args);
    if let Some(req) = args.get_mut("request") {
        strip_client_secrets(req);
    }
    let default_provider = state.secrets.default_provider().await;
    fill_keys_from_secrets(&state.secrets, &default_provider, args).await;
    if let Some(req) = args.get_mut("request") {
        fill_keys_from_secrets(&state.secrets, &default_provider, req).await;
    }
}

fn strip_client_secrets(obj: &mut Value) {
    let Some(map) = obj.as_object_mut() else {
        return;
    };
    for key in [
        "api_key",
        "apiKey",
        "canopy_api_key",
        "canopyApiKey",
        "dataforseo_login",
        "dataforseoLogin",
        "dataforseo_password",
        "dataforseoPassword",
    ] {
        map.remove(key);
    }
}

async fn fill_keys_from_secrets(
    secrets: &loremetry_core::platform_secrets::PlatformSecrets,
    default_provider: &str,
    obj: &mut Value,
) {
    let Some(map) = obj.as_object_mut() else {
        return;
    };
    let provider = map
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or(default_provider)
        .to_string();

    let story_scoped = map.contains_key("folder")
        || map.contains_key("story_id")
        || map.get("story_id").is_some();
    let needs_llm = map.contains_key("model")
        || map.contains_key("provider")
        || map.contains_key("selected")
        || map.contains_key("force_resummarize")
        || map.contains_key("message")
        || map.contains_key("chapter_text");

    if needs_llm || story_scoped {
        map.insert(
            "api_key".into(),
            Value::String(secrets.resolve_api_key(&provider).await),
        );
    }

    if story_scoped || map.contains_key("canopy_api_key") {
        map.insert(
            "canopy_api_key".into(),
            Value::String(secrets.canopy_key().await),
        );
    }

    if story_scoped || map.contains_key("dataforseo_login") || map.contains_key("dataforseo_password")
    {
        let (l, p) = secrets.dataforseo().await;
        map.insert("dataforseo_login".into(), Value::String(l));
        map.insert("dataforseo_password".into(), Value::String(p));
    }
}
