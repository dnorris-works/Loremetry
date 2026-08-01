// analysis/show_dont_tell.rs — AI-assisted "Show Don't Tell" checker.
//
// Chapters are sent to the LLM in batched calls asking it to identify passages
// where the author *tells* the reader something (emotions, reactions,
// judgments) rather than *showing* through action, dialogue, or sensory detail.
//
// The report includes the offending text plus surrounding context so the
// author can see exactly where the problem is.

use super::{emit, err, GenreResult};
use super::chapters::extract_title;
use crate::app_ctx::AppCtx;
use crate::batch_prompt::{self, BatchChapterItem, CachedBatchItem, CRAFT_BATCH_WORD_BUDGET};
use crate::db;
use crate::documents;
use crate::manuscript_fingerprint;
use crate::prompts::{self, BibleTier};

// ── Request ──────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct ShowDontTellRequest {
    #[serde(alias = "folder")]
    pub story_id:   String,
    pub provider: String,
    pub api_key:  String,
    pub model:    String,
    #[serde(default)]
    pub bible_path: String,
}

// ── AI response shape ────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Clone, Debug)]
struct AiViolation {
    #[serde(default)]
    telling_text: String,
    #[serde(default)]
    context:      String,
    #[serde(default)]
    why:          String,
    #[serde(default)]
    severity:     String,  // "minor" | "moderate" | "major"
}

// ── Command ──────────────────────────────────────────────────────────────────

pub async fn check_show_dont_tell(app: AppCtx, request: ShowDontTellRequest) -> GenreResult {
    let cancel = crate::cancel_notify();
    tokio::select! {
        result = check_inner(app, request) => result,
        _ = cancel.notified() => err("Cancelled."),
    }
}

async fn check_inner(app: AppCtx, request: ShowDontTellRequest) -> GenreResult {
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

    emit(&app, &format!("Checking {} chapter(s) for show-don't-tell violations...", chapters.len()));

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

        // Use preprocessed text (cached by chapter source hash)
        let processed = match prompts::get_preprocessed(
            &database.pool, &request.story_id, &filename, "sdt_check", &source_hash,
        )
        .await
        {
            Some(p) => p,
            None => {
                let p = prompts::preprocess_for_sdt(content);
                let _ = prompts::store_preprocessed(
                    &database.pool, &request.story_id, &filename, "sdt_check", &p, &source_hash,
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
        "sdt_check_batch",
        "sdt_check",
        &bible,
        &request.story_id,
        Some("sdt_check"),
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
            emit(&app, &format!("  → {} — {} violation(s)", filename, violations.len()));
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

    emit(&app, &format!("✓ Show Don't Tell complete — {} violation(s) across {} chapter(s).",
        total_violations, all_findings.len()));

    // Build and save report
    let report = serde_json::json!({
        "schema": "show_dont_tell_v1",
        "note": "AI-assisted: the model identifies passages that tell instead of show. Severity is subjective — use as a prompt to revisit, not a verdict.",
        "summary": {
            "chapters_checked": chapters.len(),
            "chapters_with_violations": all_findings.len(),
            "total_violations": total_violations,
        },
        "chapters": all_findings,
    }).to_string();

    let _ = db::save_document_at(&database.pool, &request.story_id, "show_dont_tell", &report, &run_ts).await;

    GenreResult { success: true, report: String::new(), error: String::new(), run_ts }
}

// ── AI extraction ────────────────────────────────────────────────────────────

fn parse_violations(value: &serde_json::Value) -> Vec<AiViolation> {
    batch_prompt::chapter_array_field(value, "findings")
        .into_iter()
        .filter_map(|item| serde_json::from_value::<AiViolation>(item).ok())
        .filter(|v| !v.telling_text.is_empty())
        .collect()
}

// ── Suggest fix for a show-don't-tell violation ──────────────────────────────

#[derive(serde::Deserialize)]
pub struct SuggestSdtFixRequest {
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
pub struct SuggestSdtFixResult {
    pub success:     bool,
    pub suggestions: String,
    pub error:       String,
}

pub async fn suggest_sdt_fix(app: AppCtx, request: SuggestSdtFixRequest) -> SuggestSdtFixResult {
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
        "sdt_suggest",
        &request.provider,
        &request.api_key,
        &request.model,
        vars,
        Some(&request.story_id),
    )
    .await {
        Ok(suggestions) => SuggestSdtFixResult { success: true, suggestions, error: String::new() },
        Err(e) => SuggestSdtFixResult { success: false, suggestions: String::new(), error: e },
    }
}
