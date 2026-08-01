// analysis/craft_audits.rs — Generic manuscript/series craft audits from StoryAuditor catalog.
//
// Each audit uses a prompt_templates row with the same id. Output schema: craft_audit_v1.
// Group membership and series scope come from craft-report-groups.json.

use std::collections::HashMap;

use super::chapters::{extract_title, truncate_words};
use super::{emit, err, extract_json_object, GenreResult};
use crate::app_ctx::AppCtx;
use crate::craft_report_groups;
use crate::db;
use crate::documents;
use crate::prompts;

pub fn is_manuscript_audit(id: &str) -> bool {
    craft_report_groups::manuscript_craft_audit_ids()
        .iter()
        .any(|audit_id| audit_id == id)
}

pub fn is_series_audit(id: &str) -> bool {
    craft_report_groups::series_report_ids()
        .iter()
        .any(|audit_id| audit_id == id)
}

pub fn is_craft_audit(id: &str) -> bool {
    is_manuscript_audit(id) || is_series_audit(id)
}

/// Build a labeled, word-budgeted manuscript excerpt from chapter documents.
pub async fn build_manuscript_excerpt(
    db: &db::Db,
    story_id: &str,
    max_words_per_chapter: usize,
    max_total_words: usize,
) -> Result<String, String> {
    let chapters = documents::list_chapters_db(db, story_id).await?;
    if chapters.is_empty() {
        return Err("No chapter documents found. Upload manuscript chapters first.".into());
    }

    let mut parts: Vec<String> = Vec::new();
    let mut total = 0usize;
    for (i, chapter) in chapters.iter().enumerate() {
        if total >= max_total_words {
            parts.push(format!(
                "\n[… remaining chapters truncated at {} words …]",
                max_total_words
            ));
            break;
        }
        let content = chapter.content.trim();
        if content.is_empty() {
            continue;
        }
        let filename = documents::chapter_display_name(chapter);
        let title = if !chapter.title.is_empty() {
            chapter.title.clone()
        } else {
            extract_title(content).unwrap_or(filename)
        };
        let remaining = max_total_words.saturating_sub(total);
        let budget = max_words_per_chapter.min(remaining);
        let body = truncate_words(content, budget);
        let wc = body.split_whitespace().count();
        total += wc;
        parts.push(format!("### Chapter {} — {}\n\n{}", i + 1, title, body));
    }

    if parts.is_empty() {
        return Err("All chapter documents are empty.".into());
    }
    Ok(parts.join("\n\n---\n\n"))
}

async fn opening_pages_excerpt(db: &db::Db, story_id: &str, max_words: usize) -> Result<String, String> {
    let chapters = documents::list_chapters_db(db, story_id).await?;
    let Some(first) = chapters.first() else {
        return Err("No chapter documents found. Upload manuscript chapters first.".into());
    };
    let content = first.content.trim();
    if content.is_empty() {
        return Err("First chapter is empty.".into());
    }
    Ok(truncate_words(content, max_words))
}

async fn run_audit_prompt(
    app: &AppCtx,
    audit_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible: &str,
    manuscript: &str,
    story_id: Option<&str>,
) -> Result<serde_json::Value, String> {
    let mut vars = HashMap::new();
    vars.insert("bible", bible);
    vars.insert("manuscript", manuscript);
    let raw = prompts::execute_prompt(app, audit_id, provider, api_key, model, vars, story_id).await?;
    let clean = extract_json_object(&raw)
        .ok_or_else(|| format!("No JSON in {} response", audit_id))?;
    serde_json::from_str(&clean).map_err(|e| format!("JSON parse ({}): {}", audit_id, e))
}

pub async fn run_manuscript_craft_audit(
    app: &AppCtx,
    database: &db::Db,
    story_id: &str,
    audit_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible_path: &str,
) -> GenreResult {
    if !crate::stories::story_exists(database, story_id).await {
        return err("Story not found.");
    }
    if let Err(msg) = crate::ai::ai_ready(provider, api_key, model) {
        return err(&msg);
    }

    let bible = prompts::load_bible_for_story(&app.db, story_id, bible_path).await;
    emit(app, &format!("Running craft audit: {}...", audit_id));

    let manuscript = match build_manuscript_excerpt(database, story_id, 2500, 20000).await {
        Ok(m) => m,
        Err(e) => return err(&e),
    };

    let run_ts = chrono::Utc::now().to_rfc3339();
    match run_audit_prompt(
        app,
        audit_id,
        provider,
        api_key,
        model,
        &bible,
        &manuscript,
        Some(story_id),
    )
    .await
    {
        Ok(parsed) => {
            let findings = parsed.get("findings").cloned().unwrap_or(serde_json::json!([]));
            let count = findings.as_array().map(|a| a.len()).unwrap_or(0);
            let report = serde_json::json!({
                "schema": "craft_audit_v1",
                "audit_id": audit_id,
                "summary": parsed.get("summary").and_then(|v| v.as_str()).unwrap_or(""),
                "findings": findings,
            })
            .to_string();
            let _ = db::save_document_at(&database.pool, story_id, audit_id, &report, &run_ts).await;
            emit(app, &format!("✓ {} — {} finding(s).", audit_id, count));
            GenreResult {
                success: true,
                report,
                error: String::new(),
                run_ts,
            }
        }
        Err(e) => {
            emit(app, &format!("✗ {}: {}", audit_id, e));
            err(&e)
        }
    }
}

pub async fn run_series_craft_audit(
    app: &AppCtx,
    database: &db::Db,
    series_id: i64,
    audit_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible_path: &str,
) -> GenreResult {
    if series_id <= 0 {
        return err("Select a series for this audit.");
    }
    if let Err(msg) = crate::ai::ai_ready(provider, api_key, model) {
        return err(&msg);
    }

    let books = match db::list_series_books(&database.pool, series_id).await {
        Ok(b) if !b.is_empty() => b,
        Ok(_) => return err("Series has no books."),
        Err(e) => return err(&e),
    };

    emit(
        app,
        &format!(
            "Running series craft audit: {} ({} books)...",
            audit_id,
            books.len()
        ),
    );

    let mut parts: Vec<String> = Vec::new();
    let mut total = 0usize;
    const MAX_TOTAL: usize = 24000;
    const PER_BOOK: usize = 8000;

    for book in &books {
        if total >= MAX_TOTAL {
            break;
        }
        let excerpt = match build_manuscript_excerpt(
            database,
            &book.story_id,
            2000,
            PER_BOOK.min(MAX_TOTAL - total),
        )
        .await
        {
            Ok(e) => e,
            Err(e) => {
                emit(app, &format!("  ⚠ {}: {}", book.story_name, e));
                continue;
            }
        };
        let wc = excerpt.split_whitespace().count();
        total += wc;
        parts.push(format!(
            "## Book {} — {}\n\n{}",
            book.book_order, book.story_name, excerpt
        ));
    }

    if parts.is_empty() {
        return err("Could not load any series manuscripts.");
    }

    // Bible: first book with a bible, or explicit path
    let mut bible = String::new();
    for book in &books {
        bible = prompts::load_bible_for_story(&app.db, &book.story_id, bible_path).await;
        if !bible.is_empty() {
            break;
        }
    }

    let manuscript = parts.join("\n\n==========\n\n");
    let save_story_id = &books[0].story_id;
    let run_ts = chrono::Utc::now().to_rfc3339();

    match run_audit_prompt(
        app,
        audit_id,
        provider,
        api_key,
        model,
        &bible,
        &manuscript,
        Some(save_story_id),
    )
    .await
    {
        Ok(parsed) => {
            let findings = parsed.get("findings").cloned().unwrap_or(serde_json::json!([]));
            let count = findings.as_array().map(|a| a.len()).unwrap_or(0);
            let report = serde_json::json!({
                "schema": "craft_audit_v1",
                "audit_id": audit_id,
                "series_id": series_id,
                "summary": parsed.get("summary").and_then(|v| v.as_str()).unwrap_or(""),
                "findings": findings,
            })
            .to_string();
            // Persist on every book in the series so each story's reports list shows it.
            for book in &books {
                let _ = db::save_document_at(
                    &database.pool,
                    &book.story_id,
                    audit_id,
                    &report,
                    &run_ts,
                )
                .await;
            }
            emit(app, &format!("✓ {} — {} finding(s).", audit_id, count));
            GenreResult {
                success: true,
                report,
                error: String::new(),
                run_ts,
            }
        }
        Err(e) => {
            emit(app, &format!("✗ {}: {}", audit_id, e));
            err(&e)
        }
    }
}

/// Used by publish hook_strength — first ~1500 words.
pub async fn build_opening_excerpt(db: &db::Db, story_id: &str) -> Result<String, String> {
    opening_pages_excerpt(db, story_id, 1500).await
}
