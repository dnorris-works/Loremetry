//! Generic Tauri-compatible invoke bridge: `POST /api/invoke` with `{ cmd, args }`.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use manuscript_intel_core::analysis::{
    ai_isms, continuity, pipeline, show_dont_tell, AnalyzeStoryRequest, FolderRequest,
};
use manuscript_intel_core::canopy::{self, MarketIntelRequest};
use manuscript_intel_core::commands::{self, ChatRequest, CostEstimateRequest};
use manuscript_intel_core::dataforseo;
use manuscript_intel_core::db;
use manuscript_intel_core::documents::{self, UpsertDocumentRequest};
use manuscript_intel_core::series::{self, CreateSeriesRequest, UpdateSeriesRequest};
use manuscript_intel_core::stories::{self, InitStoryRequest, UpdateStoryRequest};
use manuscript_intel_core::winningcat;
use manuscript_intel_core::{cancel_operation, Config};
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
    State(state): State<AppState>,
    Json(body): Json<InvokeBody>,
) -> impl IntoResponse {
    let args = normalize_args(body.args);
    match dispatch(&state, &body.cmd, args).await {
        Ok(v) => ok_json(v),
        Err(e) => json_error(e),
    }
}

async fn dispatch(state: &AppState, cmd: &str, mut args: Value) -> Result<Value, String> {
    fill_api_keys(state.config.as_ref(), &mut args);
    let app = state.ctx.clone();
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
            to_val(pipeline::analyze_story(app, request).await)
        }
        "run_craft_pipeline" => {
            let request: pipeline::CraftPipelineRequest = take_request(&args)?;
            to_val(pipeline::run_craft_pipeline(app, request).await)
        }
        "run_market_intel" => {
            let request: MarketIntelRequest = take_request(&args)?;
            to_val(canopy::run_market_intel(app, request).await)
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
            cancel_operation();
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
        "list_reports_cmd" => {
            let folder = take_story_id(&args)?;
            to_val(db::list_reports_cmd(&db, folder).await?)
        }

        // ── Settings / external APIs ─────────────────────────────────────────
        "list_models" => {
            let provider = take_string(&args, &["provider"])?;
            let api_key = take_string(&args, &["api_key"]).unwrap_or_default();
            let api_key = state.config.resolve_api_key(&provider, &api_key);
            to_val(commands::list_models(&db, provider, api_key).await?)
        }
        "test_canopy_connection" => {
            let key = take_string(&args, &["api_key", "canopy_api_key"]).unwrap_or_default();
            let key = state.config.resolve_canopy_key(&key);
            to_val(canopy::test_canopy_connection(key).await)
        }
        "test_dataforseo_connection" => {
            let login = take_string(&args, &["login"]).unwrap_or_default();
            let password = take_string(&args, &["password"]).unwrap_or_default();
            let (login, password) = state.config.resolve_dataforseo(&login, &password);
            to_val(dataforseo::test_dataforseo_connection(login, password).await)
        }
        "import_winningcat_csv" => {
            let csv_text = take_string(&args, &["csv_text", "csv"])?;
            to_val(winningcat::import_winningcat_csv(app, csv_text).await)
        }
        "remove_stale_kdp_categories" => {
            let since = take_string(&args, &["since"])?;
            to_val(winningcat::remove_stale_kdp_categories(app, since).await)
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
            let mut request: ChatRequest = take_request(&args)?;
            request.api_key = state
                .config
                .resolve_api_key(&request.provider, &request.api_key);
            match commands::chat_with_context(&db, request).await {
                Ok(r) => to_val(r),
                Err(_) => Err("Chat failed.".into()),
            }
        }
        "estimate_report_costs" => {
            let request: CostEstimateRequest = take_request(&args)?;
            to_val(commands::estimate_report_costs(app, request).await?)
        }

        // ── Misc analysis commands ───────────────────────────────────────────
        "analyze_csv" => {
            let request: commands::CsvRequest = take_request(&args)?;
            to_val(commands::analyze_csv(app, request).await)
        }
        "get_genre_taxonomy" => to_val(manuscript_intel_core::genre_taxonomy::get_genre_taxonomy(&db).await?),
        "list_genres_cmd" => to_val(db::list_genres_cmd(&db).await?),

        other => Err(format!("Unknown command: {other}")),
    }
}

// ── Document helpers (replacing filesystem create/delete) ────────────────────

async fn create_story_document(state: &AppState, args: &Value) -> Result<Value, String> {
    let req_val = args.get("request").unwrap_or(args);
    let story_id = take_string(req_val, &["story_folder", "story_id", "folder"])?;
    let name = take_string(req_val, &["name"])?;
    let location = take_string(req_val, &["location"]).unwrap_or_default();

    if !stories::story_exists(&state.ctx.db, &story_id) {
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

    let conn = state.ctx.db.0.lock().map_err(|e| e.to_string())?;
    let doc = documents::upsert_document(&conn, &upsert)?;
    Ok(json!({
        "path": format!("doc:{}", doc.id),
        "title": doc.title,
    }))
}

async fn delete_story_document(state: &AppState, args: &Value) -> Result<Value, String> {
    let story_id = take_string(args, &["story_folder", "story_id", "folder"])?;
    let doc_id = take_doc_id(args, &["file_path", "doc_id", "id", "path"])?;
    let conn = state.ctx.db.0.lock().map_err(|e| e.to_string())?;
    documents::delete_document(&conn, &story_id, doc_id)?;
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
    take_string(args, &["story_id", "folder", "id"]).or_else(|_| {
        if let Some(req) = args.get("request") {
            take_string(req, &["story_id", "folder", "id"])
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

/// Fill empty api_key / canopy / dataforseo fields from Config.
fn fill_api_keys(config: &Config, args: &mut Value) {
    fill_keys_in_obj(config, args);
    if let Some(req) = args.get_mut("request") {
        fill_keys_in_obj(config, req);
    }
}

fn fill_keys_in_obj(config: &Config, obj: &mut Value) {
    let Some(map) = obj.as_object_mut() else {
        return;
    };
    let provider = map
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or(&config.default_provider)
        .to_string();

    let needs_api = map.contains_key("api_key")
        || map.contains_key("provider")
        || map.contains_key("model");
    if needs_api {
        let current = map
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if current.trim().is_empty() {
            map.insert(
                "api_key".into(),
                Value::String(config.resolve_api_key(&provider, "")),
            );
        }
    }

    if map.contains_key("canopy_api_key") {
        let current = map
            .get("canopy_api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if current.trim().is_empty() {
            map.insert(
                "canopy_api_key".into(),
                Value::String(config.resolve_canopy_key("")),
            );
        }
    }

    if map.contains_key("dataforseo_login") || map.contains_key("dataforseo_password") {
        let login = map
            .get("dataforseo_login")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let password = map
            .get("dataforseo_password")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let (l, p) = config.resolve_dataforseo(&login, &password);
        map.insert("dataforseo_login".into(), Value::String(l));
        map.insert("dataforseo_password".into(), Value::String(p));
    }
}
