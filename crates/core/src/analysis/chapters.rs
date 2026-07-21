//! analysis/chapters.rs — Phase 1: chapter-by-chapter summarization from DB documents.

use std::collections::HashMap;

use super::{emit, err, GenreResult, FolderRequest};
use crate::app_ctx::AppCtx;
use crate::db;
use crate::documents::{self, Document};
use crate::prompts;

pub async fn generate_summaries(app: AppCtx, request: FolderRequest) -> GenreResult {
    if !crate::stories::story_exists(&app.db, &request.story_id) {
        return err("Story not found.");
    }

    crate::reset_cancel();
    let chapters = match documents::list_chapters_db(&app.db, &request.story_id) {
        Ok(c) => c,
        Err(e) => return err(&e),
    };
    if chapters.is_empty() {
        return err("No chapter documents found. Upload manuscript chapters first.");
    }

    emit(
        &app,
        &format!("Found {} chapter(s). Starting summaries...", chapters.len()),
    );

    let database = app.db.as_ref();
    let (done, skipped) = phase1_summaries(
        &app,
        database,
        &chapters,
        &request.story_id,
        &request.provider,
        &request.api_key,
        &request.model,
    )
    .await;

    GenreResult {
        success: true,
        report: format!("\u{2713} {} summarized, {} already done.", done, skipped),
        error: String::new(),
        run_ts: String::new(),
    }
}

pub(crate) async fn phase1_summaries(
    app: &AppCtx,
    database: &db::Db,
    chapters: &[Document],
    story_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
) -> (usize, usize) {
    let mut done = 0usize;
    let mut skipped = 0usize;

    for (i, chapter) in chapters.iter().enumerate() {
        let fname = documents::chapter_display_name(chapter);

        let already_done = {
            let conn = database.0.lock().unwrap();
            db::chapter_summary_exists(&conn, story_id, &fname)
        };
        if already_done {
            emit(
                app,
                &format!("  [{}/{}] SKIP: {}", i + 1, chapters.len(), fname),
            );
            skipped += 1;
            continue;
        }

        emit(
            app,
            &format!("  [{}/{}] Summarizing: {}", i + 1, chapters.len(), fname),
        );

        let content = chapter.content.trim();
        if content.is_empty() {
            emit(app, "    \u{26a0} Empty \u{2014} skipping.");
            continue;
        }

        let word_count = content.split_whitespace().count();
        emit(app, &format!("    {} words", word_count));

        match summarize_chapter(
            database,
            provider,
            api_key,
            model,
            story_id,
            &fname,
            &truncate_words(content, 8000),
        )
        .await
        {
            Ok(signals) => {
                let title = if !chapter.title.is_empty() {
                    chapter.title.clone()
                } else {
                    extract_title(content).unwrap_or_else(|| fname.clone())
                };
                let conn = database.0.lock().unwrap();
                let _ = db::save_chapter_summary(
                    &conn,
                    story_id,
                    &fname,
                    &title,
                    &signals,
                    word_count as i64,
                );
                emit(
                    app,
                    &format!("    \u{2713} Done ({} signal chars)", signals.len()),
                );
                done += 1;
            }
            Err(e) => emit(app, &format!("    \u{26a0} AI error: {}", e)),
        }

        if crate::is_cancelled() {
            emit(app, "\u{26a0} Cancelled.");
            break;
        }
    }

    emit(
        app,
        &format!("Phase 1 complete \u{2014} {} new, {} skipped.", done, skipped),
    );
    (done, skipped)
}

pub(crate) async fn summarize_chapter(
    db: &db::Db,
    provider: &str,
    api_key: &str,
    model: &str,
    story_id: &str,
    filename: &str,
    content: &str,
) -> Result<String, String> {
    let bible = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        documents::load_bible_text(&conn, story_id)
    };
    let title = extract_title(content).unwrap_or_else(|| filename.to_string());

    let mut vars = HashMap::new();
    vars.insert("chapter_title", title.as_str());
    vars.insert("chapter_text", content);
    vars.insert("bible", bible.as_str());

    prompts::execute_prompt(db, "chapter_summary", provider, api_key, model, vars).await
}

pub(crate) fn truncate_words(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        return text.to_string();
    }
    words[..max_words].join(" ") + "\n\n[...truncated...]"
}

pub(crate) fn extract_title(content: &str) -> Option<String> {
    for line in content.lines().take(20) {
        let t = line.trim();
        if t.starts_with('#') {
            return Some(t.trim_start_matches('#').trim().to_string());
        }
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    None
}

/// Combine chapter summaries into a single context string for genre analysis.
pub(crate) fn build_combined_context(summaries: &[db::ChapterSummaryRow]) -> String {
    summaries
        .iter()
        .map(|s| {
            format!(
                "## {} ({})\n\n{}",
                if s.title.is_empty() { &s.file } else { &s.title },
                s.file,
                s.signals
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}
