// analysis/ai_isms.rs — AI-assisted check for AI-sounding prose habits.
//
// Mirrors Show Don't Tell: batched LLM scan, JSON report with flagged
// passages + context, plus a suggest-fix command for rewrites.

use super::{emit, err, GenreResult};
use super::chapters::extract_title;
use crate::app_ctx::AppCtx;
use crate::batch_prompt::{self, BatchChapterItem, CachedBatchItem, CRAFT_BATCH_WORD_BUDGET};
use crate::db;
use crate::documents;
use crate::manuscript_fingerprint;
use crate::prompts::{self, BibleTier};

#[derive(serde::Deserialize)]
pub struct AiIsmsRequest {
    #[serde(alias = "folder")]
    pub story_id:   String,
    pub provider: String,
    pub api_key:  String,
    pub model:    String,
    #[serde(default)]
    pub bible_path: String,
}

#[derive(serde::Deserialize, Clone, Debug)]
struct AiViolation {
    #[serde(default)]
    telling_text: String,
    #[serde(default)]
    context:      String,
    #[serde(default)]
    why:          String,
    #[serde(default)]
    severity:     String,
}

pub async fn check_ai_isms(app: AppCtx, request: AiIsmsRequest) -> GenreResult {
    let cancel = crate::cancel_notify();
    tokio::select! {
        result = check_inner(app, request) => result,
        _ = cancel.notified() => err("Cancelled."),
    }
}

async fn check_inner(app: AppCtx, request: AiIsmsRequest) -> GenreResult {
    if !crate::stories::story_exists(&app.db, &request.story_id).await {
        return err("Story not found.");
    }
    if let Err(msg) = crate::ai::ai_ready(&request.provider, &request.api_key, &request.model) {
        return err(&msg);
    }

    crate::reset_cancel();
    let database = app.db.as_ref();
    let run_ts = chrono::Utc::now().to_rfc3339();

    let chapters = match documents::list_chapters_db(&app.db, &request.story_id).await {
        Ok(c) => c,
        Err(e) => return err(&e),
    };
    if chapters.is_empty() { return err("No chapter documents found. Upload manuscript chapters first."); }

    let bible = prompts::load_bible_tiered(&app.db, &request.story_id, &request.bible_path, BibleTier::Minimal).await;

    emit(&app, &format!("Checking {} chapter(s) for AI-isms...", chapters.len()));

    let mut chapter_meta: Vec<(usize, String, String)> = Vec::new();
    let mut batch_items: Vec<CachedBatchItem> = Vec::new();

    for (i, chapter) in chapters.iter().enumerate() {
        let content = chapter.content.trim();
        if content.is_empty() { continue; }

        let filename = documents::chapter_display_name(chapter);
        let title = if !chapter.title.is_empty() {
            chapter.title.clone()
        } else {
            extract_title(content).unwrap_or_else(|| filename.clone())
        };

        let cleaned = manuscript_fingerprint::clean_for_ai(content);
        let source_hash = manuscript_fingerprint::chapter_source_hash(&cleaned);

        let processed = match prompts::get_preprocessed(
            &database.pool, &request.story_id, &filename, "ai_isms_check", &source_hash,
        )
        .await
        {
            Some(p) => p,
            None => {
                let p = prompts::preprocess_for_ai_isms(content);
                let _ = prompts::store_preprocessed(
                    &database.pool, &request.story_id, &filename, "ai_isms_check", &p, &source_hash,
                )
                .await;
                p
            }
        };

        chapter_meta.push((i, filename.clone(), title.clone()));
        batch_items.push(CachedBatchItem {
            item: BatchChapterItem { file: filename, title, text: processed },
            source_hash,
        });
    }

    let results = batch_prompt::process_chapters_batched(
        &app,
        database,
        &request.provider,
        &request.api_key,
        &request.model,
        "ai_isms_check_batch",
        "ai_isms_check",
        &bible,
        &request.story_id,
        Some("ai_isms_check"),
        batch_items,
        CRAFT_BATCH_WORD_BUDGET,
        &[],
    )
    .await;

    let mut all_findings: Vec<serde_json::Value> = Vec::new();
    let mut total_violations = 0usize;

    for (i, filename, title) in chapter_meta {
        if crate::is_cancelled() { return err("Cancelled."); }

        let violations = match results.get(&filename) {
            Some(value) => parse_violations(value),
            None => {
                emit(&app, &format!("  ⚠ {}: no response", filename));
                Vec::new()
            }
        };

        if violations.is_empty() {
            emit(&app, &format!("  ✓ {} — clean", filename));
        } else {
            emit(&app, &format!("  → {} — {} flag(s)", filename, violations.len()));
            total_violations += violations.len();

            all_findings.push(serde_json::json!({
                "file": filename,
                "title": title,
                "chapter_index": i,
                "violations": violations.iter().map(|v| serde_json::json!({
                    "telling_text": v.telling_text,
                    "context": v.context,
                    "why": v.why,
                    "severity": v.severity,
                })).collect::<Vec<_>>(),
            }));
        }
    }

    emit(&app, &format!("✓ AI-isms complete — {} flag(s) across {} chapter(s).",
        total_violations, all_findings.len()));

    let report = serde_json::json!({
        "schema": "ai_isms_v1",
        "note": "AI-assisted: the model flags prose habits that often read as machine-generated. Severity is subjective — use as a prompt to revise, not a verdict.",
        "summary": {
            "chapters_checked": chapters.len(),
            "chapters_with_violations": all_findings.len(),
            "total_violations": total_violations,
        },
        "chapters": all_findings,
    }).to_string();

    let _ = db::save_document_at(&database.pool, &request.story_id, "ai_isms", &report, &run_ts).await;

    GenreResult { success: true, report: String::new(), error: String::new(), run_ts }
}

fn parse_violations(value: &serde_json::Value) -> Vec<AiViolation> {
    batch_prompt::chapter_array_field(value, "findings")
        .into_iter()
        .filter_map(|item| serde_json::from_value::<AiViolation>(item).ok())
        .filter(|v| !v.telling_text.is_empty())
        .collect()
}

#[derive(serde::Deserialize)]
pub struct SuggestAiIsmsFixRequest {
    pub provider:      String,
    pub api_key:       String,
    pub model:         String,
    pub telling_text:  String,
    pub context:       String,
    pub why:           String,
    pub chapter_title: String,
    #[serde(default)]
    #[serde(alias = "folder")]
    pub story_id:        String,
    #[serde(default)]
    pub bible_path:    String,
}

#[derive(serde::Serialize)]
pub struct SuggestAiIsmsFixResult {
    pub success:     bool,
    pub suggestions: String,
    pub error:       String,
}

pub async fn suggest_ai_isms_fix(app: AppCtx, request: SuggestAiIsmsFixRequest) -> SuggestAiIsmsFixResult {
    use std::collections::HashMap;

    let database = app.db.as_ref();
    let bible = crate::prompts::load_bible_for_story(&app.db, &request.story_id, &request.bible_path).await;

    let mut vars = HashMap::new();
    vars.insert("chapter_title", request.chapter_title.as_str());
    vars.insert("telling_text", request.telling_text.as_str());
    vars.insert("context", request.context.as_str());
    vars.insert("why", request.why.as_str());
    vars.insert("bible", bible.as_str());

    match crate::prompts::execute_prompt(
        &app,
        "ai_isms_suggest",
        &request.provider,
        &request.api_key,
        &request.model,
        vars,
        Some(&request.story_id),
    )
    .await {
        Ok(suggestions) => SuggestAiIsmsFixResult { success: true, suggestions, error: String::new() },
        Err(e) => SuggestAiIsmsFixResult { success: false, suggestions: String::new(), error: e },
    }
}
