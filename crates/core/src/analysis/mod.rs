// analysis/mod.rs — Shared types, helpers, and re-exports for the analysis pipeline.
//
// Sub-modules:
//   chapters   — Phase 1: chapter-by-chapter summarization
//   genres     — Genre ranking and genre analysis (Phase 2)
//   categories — Category finding, matching, and verification
//   keywords   — KDP keyword optimization, search terms, discovery keywords
//   bisac      — BISAC subject-code classification
//   pipeline   — Orchestration commands that compose the full analysis pipeline
//   zeigarnik  — Zeigarnik-effect proxy detector (Craft platform, no AI)
//   continuity — Continuity checker (Craft platform, AI-assisted, manuscript or series scope)

pub mod chapters;
pub mod genres;
pub mod categories;
pub mod keywords;
pub mod bisac;
pub mod pipeline;
pub mod zeigarnik;
pub mod readability;
pub mod continuity;
pub mod show_dont_tell;
pub mod ai_isms;
pub mod craft_audits;
pub mod publish_audits;
pub mod chapter_stats;
pub mod content_advisory;
pub mod craft_prose_checks;

use serde::{Deserialize, Serialize};
use crate::app_ctx::AppCtx;

// ── Shared result / request types ───────────────────────────────────────────

#[derive(Serialize)]
pub struct GenreResult {
    pub success: bool,
    pub report:  String,
    pub error:   String,
    #[serde(default)]
    pub run_ts:  String,
}

#[derive(Deserialize)]
pub struct FolderRequest {
    #[serde(alias = "folder")]
    pub story_id: String,
    #[serde(default)]
    pub api_key:  String,
    #[serde(default)]
    pub model:    String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub canopy_api_key: String,
    #[serde(default)]
    pub genre_model: String,
    /// Optional per-function model slot; falls back to `model`.
    #[serde(default)]
    pub summaries_model: String,
}

#[derive(Deserialize, serde::Serialize)]
pub struct AnalyzeStoryRequest {
    #[serde(alias = "folder")]
    pub story_id:            String,
    #[serde(default)]
    pub provider:          String,
    #[serde(default)]
    pub api_key:           String,
    #[serde(default)]
    pub model:             String,
    #[serde(default)]
    pub force_resummarize: bool,
    #[serde(default)]
    pub canopy_api_key:    String,
    #[serde(default)]
    pub platform:          String,  // "kdp" or "wide" (wide remapped to kdp for gating)
    #[serde(default)]
    pub dataforseo_login:  String,
    #[serde(default)]
    pub dataforseo_password: String,
    #[serde(default)]
    pub run_time:          String,  // local datetime from when user clicked the button
    #[serde(default)]
    pub selected:          Vec<String>,
    #[serde(default = "default_true")]
    pub publish_ebook:     bool,
    #[serde(default = "default_true")]
    pub publish_print:     bool,
    #[serde(default)]
    pub genre_model:       String,
    #[serde(default)]
    pub summaries_model:   String,
}

fn default_true() -> bool { true }

// ── Shared helpers ──────────────────────────────────────────────────────────

pub fn emit(app: &AppCtx, msg: &str) {
    let _ = app.emit("genre:log", msg);
}

pub fn err(msg: &str) -> GenreResult {
    GenreResult { success: false, report: String::new(), error: msg.to_string(), run_ts: String::new() }
}

/// Extract a JSON array or object from text that may have markdown fencing or preamble.
pub fn extract_json_object(text: &str) -> Option<String> {
    let stripped = text.trim()
        .trim_start_matches("```json").trim_start_matches("```")
        .trim_end_matches("```").trim();
    if serde_json::from_str::<serde_json::Value>(stripped).is_ok() {
        return Some(stripped.to_string());
    }
    let start = text.find('{')?;
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes[start..].iter().enumerate() {
        if escape { escape = false; continue; }
        match b {
            b'\\' if in_string => escape = true,
            b'"'  => in_string = !in_string,
            b'{'  if !in_string => depth += 1,
            b'}'  if !in_string => {
                depth -= 1;
                if depth == 0 {
                    let candidate = &text[start..start+i+1];
                    if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                        return Some(candidate.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

// ── Re-exports of Tauri commands ────────────────────────────────────────────
// (Commands are referenced by full path in lib.rs generate_handler![])
