//! analysis/chapters.rs — Phase 1: per-chapter AI genre-signal summaries.
//!
//! Change detection uses source_hash only. Summaries are prose stored in
//! chapter_summaries.signals and fed to book-level genre analysis.
//!
//! Chapters come from the `story_assets` manuscript slot (see `documents`),
//! not from the filesystem, and are summarized in batched AI calls.

use super::chapter_stats::ChapterFingerprint;
use super::{emit, err, FolderRequest, GenreResult};
use crate::app_ctx::AppCtx;
use crate::batch_prompt::{self, BatchChapterItem, CachedBatchItem, DEFAULT_WORD_BUDGET};
use crate::db;
use crate::documents::{self, Document};
use crate::manuscript_fingerprint::{chapter_source_hash, clean_for_ai};
use crate::prompts;

/// Max words sent to the summary model (~full chapter for typical manuscripts).
pub const CHAPTER_SUMMARY_WORD_LIMIT: usize = 1500;
/// SDT / AI-isms excerpt limit (words).
pub const CRAFT_EXCERPT_WORD_LIMIT: usize = 2500;
/// Continuity extract excerpt limit (words).
pub const CONTINUITY_EXCERPT_WORD_LIMIT: usize = 5000;

pub struct Phase1Config<'a> {
    pub provider:        &'a str,
    pub api_key:         &'a str,
    pub summaries_model: &'a str,
    pub default_model:   &'a str,
    pub force:           bool,
}

impl Phase1Config<'_> {
    pub fn resolve_summaries_model(&self) -> Result<String, String> {
        crate::ai::resolve_slot_model(self.summaries_model, self.default_model)
    }
}

pub fn phase1_config_from<'a>(
    provider: &'a str,
    api_key: &'a str,
    model: &'a str,
    summaries_model: &'a str,
    force: bool,
) -> Phase1Config<'a> {
    Phase1Config {
        provider,
        api_key,
        summaries_model,
        default_model: model,
        force,
    }
}

// ── Command ──────────────────────────────────────────────────────────────────

pub async fn generate_summaries(app: AppCtx, request: FolderRequest) -> GenreResult {
    if !crate::stories::story_exists(&app.db, &request.story_id).await {
        return err("Story not found.");
    }

    crate::reset_cancel();
    let chapters = match documents::list_chapters_db(&app.db, &request.story_id).await {
        Ok(c) => c,
        Err(e) => return err(&e),
    };
    if chapters.is_empty() {
        return err("No chapter documents found. Upload manuscript chapters first.");
    }

    let config = phase1_config_from(
        &request.provider,
        &request.api_key,
        &request.model,
        &request.summaries_model,
        false,
    );
    if let Err(msg) = validate_phase1_ai(&config) {
        return err(&msg);
    }

    emit(
        &app,
        &format!(
            "Found {} chapter document(s). Summarizing genre signals...",
            chapters.len()
        ),
    );

    let database = app.db.as_ref();
    let (done, skipped) =
        phase1_summaries(&app, database, &chapters, &request.story_id, &config).await;

    let run_ts = chrono::Utc::now().to_rfc3339();
    let manuscript_fp = crate::manuscript_fingerprint::compute_manuscript_fingerprint(&chapters);
    let _ = db::record_artifact_built(
        &database.pool,
        &request.story_id,
        "summaries",
        &manuscript_fp,
    )
    .await;
    let _ = db::sync_manuscript_state(&database.pool, &request.story_id, &manuscript_fp).await;

    GenreResult {
        success: true,
        report: format!(
            "\u{2713} {} summarized, {} already up to date.",
            done, skipped
        ),
        error: String::new(),
        run_ts,
    }
}

fn validate_phase1_ai(config: &Phase1Config<'_>) -> Result<(), String> {
    let model = config.resolve_summaries_model()?;
    crate::ai::ai_ready(config.provider, config.api_key, &model)
}

// ── Phase 1 implementation ───────────────────────────────────────────────────

/// Whether any chapter is missing or stale relative to its current source hash.
pub async fn any_chapter_needs_summary(
    pool: &sqlx::PgPool,
    story_id: &str,
    chapters: &[Document],
) -> bool {
    for chapter in chapters {
        let fname = documents::chapter_display_name(chapter);
        let cleaned = clean_for_ai(&chapter.content);
        if cleaned.is_empty() {
            continue;
        }
        let source_hash = chapter_source_hash(&cleaned);
        if !db::chapter_has_current_summary(pool, story_id, &fname, &source_hash).await {
            return true;
        }
    }
    false
}

pub(crate) async fn phase1_summaries(
    app: &AppCtx,
    database: &db::Db,
    chapters: &[Document],
    story_id: &str,
    config: &Phase1Config<'_>,
) -> (usize, usize) {
    let mut done = 0usize;
    let mut skipped = 0usize;

    let summaries_model = match config.resolve_summaries_model() {
        Ok(m) => m,
        Err(e) => {
            emit(app, &format!("\u{26a0} {}", e));
            return (0, 0);
        }
    };
    if let Err(e) = crate::ai::ai_ready(config.provider, config.api_key, &summaries_model) {
        emit(app, &format!("\u{26a0} {}", e));
        return (0, 0);
    }

    emit(app, &format!("  Using model: {} (provider: {})", summaries_model, config.provider));

    struct PendingSummary {
        item:        BatchChapterItem,
        source_hash: String,
        word_count:  i64,
    }

    let bible = prompts::load_bible_tiered(&app.db, story_id, "", prompts::BibleTier::Medium).await;
    let mut pending: Vec<PendingSummary> = Vec::new();

    for (i, chapter) in chapters.iter().enumerate() {
        let fname = documents::chapter_display_name(chapter);

        let content = chapter.content.trim();
        if content.is_empty() {
            emit(
                app,
                &format!("  [{}/{}] SKIP (empty): {}", i + 1, chapters.len(), fname),
            );
            emit_summary_progress(app, &fname, "skipped");
            continue;
        }

        let cleaned_source = clean_for_ai(content);
        if cleaned_source.is_empty() {
            emit(
                app,
                &format!(
                    "  [{}/{}] SKIP (empty after cleanup): {}",
                    i + 1,
                    chapters.len(),
                    fname
                ),
            );
            emit_summary_progress(app, &fname, "skipped");
            continue;
        }

        let source_hash = chapter_source_hash(&cleaned_source);
        if !config.force
            && db::chapter_has_current_summary(&database.pool, story_id, &fname, &source_hash).await
        {
            emit(
                app,
                &format!(
                    "  [{}/{}] SKIP (up to date): {}",
                    i + 1,
                    chapters.len(),
                    fname
                ),
            );
            emit_summary_progress(app, &fname, "skipped");
            skipped += 1;
            continue;
        }

        let title = if !chapter.title.is_empty() {
            chapter.title.clone()
        } else {
            extract_title(content)
                .or_else(|| extract_title(&cleaned_source))
                .unwrap_or_else(|| fname.clone())
        };
        let word_count = cleaned_source.split_whitespace().count() as i64;
        let chapter_text = truncate_words(&cleaned_source, CHAPTER_SUMMARY_WORD_LIMIT);

        pending.push(PendingSummary {
            item: BatchChapterItem {
                file:  fname,
                title,
                text:  chapter_text,
            },
            source_hash,
            word_count,
        });
    }

    if !pending.is_empty() {
        emit(
            app,
            &format!(
                "  Summarizing {} chapter(s) in batched AI calls...",
                pending.len()
            ),
        );

        for p in &pending {
            emit_summary_progress(app, &p.item.file, "started");
        }

        let batch_items: Vec<CachedBatchItem> = pending
            .iter()
            .map(|p| CachedBatchItem {
                item:        p.item.clone(),
                source_hash: p.source_hash.clone(),
            })
            .collect();
        let results = batch_prompt::process_chapters_batched(
            app,
            database,
            config.provider,
            config.api_key,
            &summaries_model,
            "chapter_summary_batch",
            "chapter_summary",
            &bible,
            story_id,
            None,
            batch_items,
            DEFAULT_WORD_BUDGET,
            &[],
        )
        .await;

        for p in pending {
            if crate::is_cancelled() {
                emit(app, "\u{26a0} Cancelled.");
                break;
            }

            let Some(value) = results.get(&p.item.file) else {
                emit(
                    app,
                    &format!("    \u{26a0} {} — no summary returned", p.item.file),
                );
                continue;
            };

            let Some(signals) = render_genre_signals_from_chapter_value(value) else {
                emit(app, &format!("    \u{26a0} {} — empty summary", p.item.file));
                continue;
            };

            match db::save_chapter_summary(
                &database.pool,
                story_id,
                &p.item.file,
                &p.item.title,
                &signals,
                &p.source_hash,
                p.word_count,
            )
            .await
            {
                Ok(()) => {
                    emit(app, &format!("    \u{2713} {} — summary saved", p.item.file));
                    emit_summary_progress(app, &p.item.file, "done");
                    done += 1;
                }
                Err(e) => emit(app, &format!("    \u{26a0} Save error: {}", e)),
            }
        }
    }

    emit(
        app,
        &format!(
            "Phase 1 complete \u{2014} {} summarized, {} skipped.",
            done, skipped
        ),
    );
    (done, skipped)
}

fn emit_summary_progress(app: &AppCtx, filename: &str, status: &str) {
    let payload = serde_json::json!({ "filename": filename, "status": status }).to_string();
    app.emit("summary:chapter-progress", &payload);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

pub(crate) fn extract_title(content: &str) -> Option<String> {
    content
        .lines()
        .take(10)
        .find(|l| l.trim().starts_with("# "))
        .map(|l| l.trim().trim_start_matches("# ").trim().to_string())
}

pub(crate) fn truncate_words(text: &str, max: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max {
        return text.to_string();
    }
    words[..max].join(" ") + "\n\n[Truncated]"
}

/// Render stored genre-signal text from a per-chapter LLM value (structured JSON or legacy prose).
pub fn render_genre_signals_from_chapter_value(value: &serde_json::Value) -> Option<String> {
    if let Some(summary) = value.get("summary") {
        if let Some(s) = render_genre_signals(summary) {
            return Some(s);
        }
    }
    render_genre_signals(value)
        .or_else(|| crate::batch_prompt::chapter_string_field(value, "summary"))
}

/// Compact one-line genre signals from structured JSON or legacy prose string.
pub fn render_genre_signals(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        serde_json::Value::Object(obj) => {
            let mut parts: Vec<String> = Vec::new();
            for (key, label) in [
                ("setting", "Setting"),
                ("tone", "Tone"),
                ("faith_market", "Faith market"),
                ("faith", "Faith"),
                ("heat", "Heat"),
                ("conflict", "Conflict"),
                ("pacing", "Pacing"),
                ("voice", "Voice"),
            ] {
                if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
                    let t = v.trim();
                    if !t.is_empty() {
                        parts.push(format!("{label}: {t}"));
                    }
                }
            }
            if let Some(tropes) = obj.get("tropes").and_then(|v| v.as_array()) {
                let list: Vec<String> = tropes
                    .iter()
                    .filter_map(|t| t.as_str().map(|s| s.trim()).filter(|s| !s.is_empty()))
                    .map(String::from)
                    .collect();
                if !list.is_empty() {
                    parts.push(format!("Tropes: {}", list.join(", ")));
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" | "))
            }
        }
        _ => None,
    }
}

/// True when `signals` holds AI prose, not a legacy fingerprint JSON blob.
pub fn is_prose_summary(signals: &str) -> bool {
    let s = signals.trim();
    if s.is_empty() {
        return false;
    }
    if ChapterFingerprint::from_storage(s).is_some() {
        return false;
    }
    true
}

/// Book-level dossier from per-chapter AI genre-signal summaries.
pub(crate) fn build_combined_context(summaries: &[db::ChapterSummaryRow]) -> String {
    const ROLLUP_CHAPTER_THRESHOLD: usize = 10;
    const ROLLUP_CHAR_THRESHOLD: usize = 14_000;
    const ROLLUP_SNIPPET_CHARS: usize = 420;

    let mut total_chars = 0usize;
    for s in summaries {
        total_chars += s.signals.len();
    }

    let use_rollup =
        summaries.len() > ROLLUP_CHAPTER_THRESHOLD || total_chars > ROLLUP_CHAR_THRESHOLD;

    let mut out = String::from(
        "Chapter genre-signal summaries for the full manuscript.\n\
         Use these to infer genre niche, subgenre, tone, faith vs secular content, heat level, and category fit.\n\n",
    );

    if use_rollup {
        out.push_str(
            "(Rollup mode: long manuscripts are condensed to key signals per chapter.)\n\n",
        );
    }

    for (i, s) in summaries.iter().enumerate() {
        let body = s.signals.trim();
        if body.is_empty() {
            continue;
        }
        let excerpt = if use_rollup {
            let snippet: String = body.chars().take(ROLLUP_SNIPPET_CHARS).collect();
            if body.chars().count() > ROLLUP_SNIPPET_CHARS {
                format!("{snippet}…")
            } else {
                snippet
            }
        } else {
            body.to_string()
        };
        out.push_str(&format!(
            "--- Chapter {} — {} (~{} words) ---\n{}\n\n",
            i + 1,
            s.title,
            s.word_count,
            excerpt
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_title_takes_markdown_heading() {
        assert_eq!(
            extract_title("# Chapter One\n\nThe night was cold."),
            Some("Chapter One".to_string())
        );
        assert_eq!(extract_title("The night was cold."), None);
    }

    #[test]
    fn truncate_words_marks_truncation() {
        let text = "a b c d e";
        assert_eq!(truncate_words(text, 5), text);
        assert_eq!(truncate_words(text, 2), "a b\n\n[Truncated]");
    }

    #[test]
    fn render_genre_signals_structured_json() {
        let v = serde_json::json!({
            "setting": "contemporary Seattle",
            "tone": "romantic suspense",
            "faith": "secular",
            "heat": "clean",
            "tropes": ["forced proximity", "small town"]
        });
        let rendered = render_genre_signals(&v).unwrap();
        assert!(rendered.contains("Faith: secular"));
        assert!(rendered.contains("forced proximity"));
    }

    #[test]
    fn render_genre_signals_from_nested_summary() {
        let v = serde_json::json!({ "summary": { "tone": "cozy mystery" } });
        assert_eq!(
            render_genre_signals_from_chapter_value(&v),
            Some("Tone: cozy mystery".to_string())
        );
    }

    #[test]
    fn render_genre_signals_from_legacy_prose() {
        let v = serde_json::json!({ "summary": "Romance with suspense elements." });
        assert_eq!(
            render_genre_signals_from_chapter_value(&v),
            Some("Romance with suspense elements.".to_string())
        );
    }

    #[test]
    fn build_combined_context_uses_prose_summaries() {
        let row = db::ChapterSummaryRow {
            file: "01.md".into(),
            title: "Ch1".into(),
            signals: "Contemporary romantic suspense. No Christian or faith themes.".into(),
            word_count: 1500,
        };
        let combined = build_combined_context(&[row]);
        assert!(combined.contains("romantic suspense"));
        assert!(combined.contains("No Christian"));
    }

    #[test]
    fn is_prose_summary_rejects_fingerprint_json() {
        let fp = super::super::chapter_stats::compute_chapter_fingerprint("T", "text");
        assert!(!is_prose_summary(&fp.to_storage_json()));
        assert!(is_prose_summary("Romance with suspense elements."));
    }
}
