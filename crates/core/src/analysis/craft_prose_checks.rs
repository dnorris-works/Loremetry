// analysis/craft_prose_checks.rs — Combined Show Don't Tell + AI-isms in one batched pass.

use super::chapters::extract_title;
use super::{emit, err, GenreResult};
use crate::app_ctx::AppCtx;
use crate::batch_prompt::{self, BatchChapterItem, CachedBatchItem, CRAFT_BATCH_WORD_BUDGET};
use crate::db;
use crate::documents;
use crate::manuscript_fingerprint;
use crate::prompts;

#[derive(serde::Deserialize)]
pub struct CraftProseChecksRequest {
    #[serde(alias = "folder")]
    pub story_id:   String,
    #[serde(default)]
    pub provider:   String,
    #[serde(default)]
    pub api_key:    String,
    #[serde(default)]
    pub model:      String,
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

pub async fn check_craft_prose_combined(app: AppCtx, request: CraftProseChecksRequest) -> GenreResult {
    let cancel = crate::cancel_notify();
    tokio::select! {
        result = check_inner(app, request) => result,
        _ = cancel.notified() => err("Cancelled."),
    }
}

async fn check_inner(app: AppCtx, request: CraftProseChecksRequest) -> GenreResult {
    if !crate::stories::story_exists(&app.db, &request.story_id).await {
        return err("Story not found.");
    }
    if request.api_key.is_empty() || request.model.is_empty() {
        return err("Craft prose checks require an API key and model. Set them in Settings.");
    }

    crate::reset_cancel();
    let database = app.db.as_ref();
    let run_ts = chrono::Utc::now().to_rfc3339();

    let chapters = match documents::list_chapters_db(&app.db, &request.story_id).await {
        Ok(c) => c,
        Err(e) => return err(&e),
    };
    if chapters.is_empty() {
        return err("No chapter documents found. Upload manuscript chapters first.");
    }

    let bible = prompts::load_bible_for_story(&app.db, &request.story_id, &request.bible_path).await;

    emit(
        &app,
        &format!(
            "Checking {} chapter(s) for show-don't-tell and AI-isms (combined pass)...",
            chapters.len()
        ),
    );

    let mut chapter_meta: Vec<(usize, String, String)> = Vec::new();
    let mut batch_items: Vec<CachedBatchItem> = Vec::new();

    for (i, chapter) in chapters.iter().enumerate() {
        let content = chapter.content.trim();
        if content.is_empty() {
            continue;
        }

        let filename = documents::chapter_display_name(chapter);
        let title = if !chapter.title.is_empty() {
            chapter.title.clone()
        } else {
            extract_title(content).unwrap_or_else(|| filename.clone())
        };

        let cleaned = manuscript_fingerprint::clean_for_ai(content);
        let source_hash = manuscript_fingerprint::chapter_source_hash(&cleaned);

        let processed = match prompts::get_preprocessed(
            &database.pool,
            &request.story_id,
            &filename,
            "craft_prose_checks",
            &source_hash,
        )
        .await
        {
            Some(p) => p,
            None => {
                let p = prompts::preprocess_for_sdt(content);
                let _ = prompts::store_preprocessed(
                    &database.pool,
                    &request.story_id,
                    &filename,
                    "craft_prose_checks",
                    &p,
                    &source_hash,
                )
                .await;
                p
            }
        };

        chapter_meta.push((i, filename.clone(), title.clone()));
        batch_items.push(CachedBatchItem {
            item: BatchChapterItem {
                file: filename,
                title,
                text: processed,
            },
            source_hash,
        });
    }

    let results = batch_prompt::process_chapters_batched(
        &app,
        database,
        &request.provider,
        &request.api_key,
        &request.model,
        "craft_prose_checks_batch",
        "craft_prose_checks_single",
        &bible,
        &request.story_id,
        Some("craft_prose_checks"),
        batch_items,
        CRAFT_BATCH_WORD_BUDGET,
        &[],
    )
    .await;

    let mut sdt_findings: Vec<serde_json::Value> = Vec::new();
    let mut ai_findings: Vec<serde_json::Value> = Vec::new();
    let mut total_sdt = 0usize;
    let mut total_ai = 0usize;

    for (i, filename, title) in chapter_meta {
        if crate::is_cancelled() {
            return err("Cancelled.");
        }

        let value = match results.get(&filename) {
            Some(v) => v,
            None => {
                emit(&app, &format!("  ⚠ {}: no response", filename));
                continue;
            }
        };

        let sdt_violations = parse_violations(value, "sdt_findings");
        let ai_violations = parse_violations(value, "ai_isms_findings");

        if sdt_violations.is_empty() && ai_violations.is_empty() {
            emit(&app, &format!("  ✓ {} — clean", filename));
        } else {
            emit(
                &app,
                &format!(
                    "  → {} — {} SDT, {} AI-ism(s)",
                    filename,
                    sdt_violations.len(),
                    ai_violations.len()
                ),
            );
        }

        total_sdt += sdt_violations.len();
        total_ai += ai_violations.len();

        if !sdt_violations.is_empty() {
            sdt_findings.push(serde_json::json!({
                "file": filename,
                "title": title,
                "chapter_index": i,
                "violations": violations_json(&sdt_violations),
            }));
        }
        if !ai_violations.is_empty() {
            ai_findings.push(serde_json::json!({
                "file": filename,
                "title": title,
                "chapter_index": i,
                "violations": violations_json(&ai_violations),
            }));
        }
    }

    emit(
        &app,
        &format!(
            "✓ Combined craft check — {} SDT + {} AI-ism flag(s).",
            total_sdt, total_ai
        ),
    );

    let sdt_report = serde_json::json!({
        "schema": "show_dont_tell_v1",
        "note": "AI-assisted: the model identifies passages that tell instead of show. Severity is subjective — use as a prompt to revisit, not a verdict.",
        "summary": {
            "chapters_checked": chapters.len(),
            "chapters_with_violations": sdt_findings.len(),
            "total_violations": total_sdt,
        },
        "chapters": sdt_findings,
    })
    .to_string();

    let ai_report = serde_json::json!({
        "schema": "ai_isms_v1",
        "note": "AI-assisted: the model flags prose habits that often read as machine-generated. Severity is subjective — use as a prompt to revise, not a verdict.",
        "summary": {
            "chapters_checked": chapters.len(),
            "chapters_with_violations": ai_findings.len(),
            "total_violations": total_ai,
        },
        "chapters": ai_findings,
    })
    .to_string();

    let _ = db::save_document_at(&database.pool, &request.story_id, "show_dont_tell", &sdt_report, &run_ts).await;
    let _ = db::save_document_at(&database.pool, &request.story_id, "ai_isms", &ai_report, &run_ts).await;

    GenreResult {
        success: true,
        report: String::new(),
        error: String::new(),
        run_ts,
    }
}

fn parse_violations(value: &serde_json::Value, field: &str) -> Vec<AiViolation> {
    batch_prompt::chapter_array_field(value, field)
        .into_iter()
        .filter_map(|item| serde_json::from_value::<AiViolation>(item).ok())
        .filter(|v| !v.telling_text.is_empty())
        .collect()
}

fn violations_json(violations: &[AiViolation]) -> Vec<serde_json::Value> {
    violations
        .iter()
        .map(|v| {
            serde_json::json!({
                "telling_text": v.telling_text,
                "context": v.context,
                "why": v.why,
                "severity": v.severity,
            })
        })
        .collect()
}
