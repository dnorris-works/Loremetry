// analysis/craft_audits.rs — Generic manuscript/series craft audits from StoryAuditor catalog.
//
// Each audit uses a prompt_templates row with the same id. Output schema: craft_audit_v1.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

use super::chapters::{collect_chapters, extract_title, truncate_words};
use super::{emit, err, extract_json_object, GenreResult};
use crate::db;
use crate::prompts;

/// Manuscript-level craft audits (single book).
pub const MANUSCRIPT_AUDITS: &[&str] = &[
    "chekhovs_gun",
    "red_herring_vs_abandoned",
    "foreshadowing_twist_fairness",
    "macguffin_clarity",
    "want_vs_need",
    "thematic_throughline",
    "mirror_foil_character",
    "pov_discipline",
    "story_beat_placement",
    "scene_sequel_balance",
    "timeline_flashback",
    "dramatic_irony",
    "stakes_escalation",
];

/// Series-scope craft audits.
pub const SERIES_AUDITS: &[&str] = &[
    "cross_book_setup_payoff",
    "series_pacing_comparator",
    "recurring_motif_theme_series",
];

pub fn is_manuscript_audit(id: &str) -> bool {
    MANUSCRIPT_AUDITS.contains(&id)
}

pub fn is_series_audit(id: &str) -> bool {
    SERIES_AUDITS.contains(&id)
}

pub fn is_craft_audit(id: &str) -> bool {
    is_manuscript_audit(id) || is_series_audit(id)
}

/// Build a labeled, word-budgeted manuscript excerpt from chapter files.
pub fn build_manuscript_excerpt(folder: &Path, max_words_per_chapter: usize, max_total_words: usize) -> Result<String, String> {
    let chapters = collect_chapters(folder);
    if chapters.is_empty() {
        return Err("No .md chapter files found.".into());
    }
    let mut parts: Vec<String> = Vec::new();
    let mut total = 0usize;
    for (i, path) in chapters.iter().enumerate() {
        if total >= max_total_words {
            parts.push(format!("\n[… remaining chapters truncated at {} words …]", max_total_words));
            break;
        }
        let content = std::fs::read_to_string(path).unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }
        let title = extract_title(&content).unwrap_or_else(|| {
            path.file_name().unwrap_or_default().to_string_lossy().to_string()
        });
        let remaining = max_total_words.saturating_sub(total);
        let budget = max_words_per_chapter.min(remaining);
        let body = truncate_words(&content, budget);
        let wc = body.split_whitespace().count();
        total += wc;
        parts.push(format!("### Chapter {} — {}\n\n{}", i + 1, title, body));
    }
    if parts.is_empty() {
        return Err("All chapter files are empty.".into());
    }
    Ok(parts.join("\n\n---\n\n"))
}

fn opening_pages_excerpt(folder: &Path, max_words: usize) -> Result<String, String> {
    let chapters = collect_chapters(folder);
    let Some(first) = chapters.first() else {
        return Err("No .md chapter files found.".into());
    };
    let content = std::fs::read_to_string(first).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Err("First chapter is empty.".into());
    }
    Ok(truncate_words(&content, max_words))
}

async fn run_audit_prompt(
    database: &db::Db,
    audit_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible: &str,
    manuscript: &str,
) -> Result<serde_json::Value, String> {
    let mut vars = HashMap::new();
    vars.insert("bible", bible);
    vars.insert("manuscript", manuscript);
    let raw = prompts::execute_prompt(database, audit_id, provider, api_key, model, vars).await?;
    let clean = extract_json_object(&raw)
        .ok_or_else(|| format!("No JSON in {} response", audit_id))?;
    serde_json::from_str(&clean).map_err(|e| format!("JSON parse ({}): {}", audit_id, e))
}

pub async fn run_manuscript_craft_audit(
    app: &AppHandle,
    database: &db::Db,
    folder: &str,
    audit_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible_path: &str,
) -> GenreResult {
    let path = PathBuf::from(folder);
    if !path.exists() {
        return err("Folder does not exist.");
    }
    if api_key.is_empty() || model.is_empty() {
        return err("This audit requires an API key and model. Set them in Settings.");
    }

    let bible = prompts::load_bible_for_story(folder, bible_path);
    emit(app, &format!("Running craft audit: {}...", audit_id));

    let manuscript = match build_manuscript_excerpt(&path, 2500, 20000) {
        Ok(m) => m,
        Err(e) => return err(&e),
    };

    match run_audit_prompt(database, audit_id, provider, api_key, model, &bible, &manuscript).await {
        Ok(parsed) => {
            let findings = parsed.get("findings").cloned().unwrap_or(serde_json::json!([]));
            let count = findings.as_array().map(|a| a.len()).unwrap_or(0);
            let report = serde_json::json!({
                "schema": "craft_audit_v1",
                "audit_id": audit_id,
                "summary": parsed.get("summary").and_then(|v| v.as_str()).unwrap_or(""),
                "findings": findings,
            }).to_string();
            let conn = database.0.lock().unwrap();
            let _ = db::save_document(&conn, folder, audit_id, &report);
            emit(app, &format!("✓ {} — {} finding(s).", audit_id, count));
            GenreResult { success: true, report, error: String::new(), run_ts: chrono::Utc::now().to_rfc3339() }
        }
        Err(e) => {
            emit(app, &format!("✗ {}: {}", audit_id, e));
            err(&e)
        }
    }
}

pub async fn run_series_craft_audit(
    app: &AppHandle,
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
    if api_key.is_empty() || model.is_empty() {
        return err("This audit requires an API key and model. Set them in Settings.");
    }

    let books = {
        let conn = database.0.lock().unwrap();
        match db::list_series_books(&conn, series_id) {
            Ok(b) if !b.is_empty() => b,
            Ok(_) => return err("Series has no books."),
            Err(e) => return err(&e),
        }
    };

    emit(app, &format!("Running series craft audit: {} ({} books)...", audit_id, books.len()));

    let mut parts: Vec<String> = Vec::new();
    let mut total = 0usize;
    const MAX_TOTAL: usize = 24000;
    const PER_BOOK: usize = 8000;

    for book in &books {
        if total >= MAX_TOTAL {
            break;
        }
        let folder = PathBuf::from(&book.story_folder);
        let excerpt = match build_manuscript_excerpt(&folder, 2000, PER_BOOK.min(MAX_TOTAL - total)) {
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
    let bible = {
        let mut b = String::new();
        for book in &books {
            b = prompts::load_bible_for_story(&book.story_folder, bible_path);
            if !b.is_empty() {
                break;
            }
        }
        b
    };

    let manuscript = parts.join("\n\n==========\n\n");
    // Save under the first book's folder so the report appears in the sidebar when that story is active.
    let save_folder = &books[0].story_folder;

    match run_audit_prompt(database, audit_id, provider, api_key, model, &bible, &manuscript).await {
        Ok(parsed) => {
            let findings = parsed.get("findings").cloned().unwrap_or(serde_json::json!([]));
            let count = findings.as_array().map(|a| a.len()).unwrap_or(0);
            let report = serde_json::json!({
                "schema": "craft_audit_v1",
                "audit_id": audit_id,
                "series_id": series_id,
                "summary": parsed.get("summary").and_then(|v| v.as_str()).unwrap_or(""),
                "findings": findings,
            }).to_string();
            // Persist on every book in the series so each story's reports list shows it.
            {
                let conn = database.0.lock().unwrap();
                for book in &books {
                    let _ = db::save_document(&conn, &book.story_folder, audit_id, &report);
                }
            }
            let _ = save_folder; // used conceptually; all books get the save
            emit(app, &format!("✓ {} — {} finding(s).", audit_id, count));
            GenreResult { success: true, report, error: String::new(), run_ts: chrono::Utc::now().to_rfc3339() }
        }
        Err(e) => {
            emit(app, &format!("✗ {}: {}", audit_id, e));
            err(&e)
        }
    }
}

/// Used by publish hook_strength — first ~1500 words.
pub fn build_opening_excerpt(folder: &str) -> Result<String, String> {
    opening_pages_excerpt(Path::new(folder), 1500)
}
