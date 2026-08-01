// analysis/publish_audits.rs — Publish-platform analyses from StoryAuditor marketing features.

use std::collections::HashMap;

use crate::manuscript_fingerprint::chapter_source_hash;
use super::chapters::{extract_title, build_combined_context};
use super::craft_audits::build_opening_excerpt;
use super::{emit, err, extract_json_object, GenreResult};
use crate::app_ctx::AppCtx;
use crate::batch_prompt::{self, BatchChapterItem, CachedBatchItem, CRAFT_BATCH_WORD_BUDGET};
use crate::db;
use crate::documents;
use crate::prompts;

fn truncate_words(text: &str, max: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max {
        return text.to_string();
    }
    words[..max].join(" ") + "\n\n[Truncated]"
}

async fn per_chapter_json(
    app: &AppCtx,
    database: &db::Db,
    story_id: &str,
    template_id: &str,
    batch_template_id: &str,
    cache_report_type: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible: &str,
    include_bible: bool,
) -> Result<Vec<serde_json::Value>, String> {
    let chapters = match documents::list_chapters_db(database, story_id).await {
        Ok(c) => c,
        Err(e) => return Err(e),
    };
    if chapters.is_empty() {
        return Err("No chapter documents found. Upload manuscript chapters first.".into());
    }

    let bible_text = if include_bible { bible } else { "" };
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
        let chapter_text = truncate_words(content, 4000);

        chapter_meta.push((i, filename.clone(), title.clone()));
        batch_items.push(CachedBatchItem {
            item: BatchChapterItem {
                file: filename,
                title,
                text: chapter_text,
            },
            source_hash: chapter_source_hash(content),
        });
    }

    emit(
        app,
        &format!(
            "  Analyzing {} chapter(s) in batched AI calls...",
            batch_items.len()
        ),
    );

    let results = batch_prompt::process_chapters_batched(
        app,
        database,
        provider,
        api_key,
        model,
        batch_template_id,
        template_id,
        bible_text,
        story_id,
        Some(cache_report_type),
        batch_items,
        CRAFT_BATCH_WORD_BUDGET,
        &[],
    )
    .await;

    let mut rows = Vec::new();
    for (i, filename, title) in chapter_meta {
        if crate::is_cancelled() {
            return Err("Cancelled.".into());
        }

        let value = results.get(&filename).cloned().ok_or_else(|| {
            format!("No JSON for {}", filename)
        })?;

        let mut parsed = if value.is_object() {
            value
        } else {
            return Err(format!("Invalid JSON object for {}", filename));
        };

        if let Some(obj) = parsed.as_object_mut() {
            obj.insert("file".into(), serde_json::json!(filename));
            obj.insert("title".into(), serde_json::json!(title));
            obj.insert("chapter_index".into(), serde_json::json!(i));
        }
        rows.push(parsed);
    }

    Ok(rows)
}

pub async fn run_ai_beta_reader(
    app: &AppCtx,
    database: &db::Db,
    story_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible_path: &str,
) -> GenreResult {
    if !crate::stories::story_exists(database, story_id).await {
        return err("Story not found.");
    }
    if api_key.is_empty() || model.is_empty() {
        return err("An API key and model are required. Set them in Settings.");
    }

    let bible = prompts::load_bible_for_story(&app.db, story_id, bible_path).await;
    emit(app, "Running AI beta reader (per chapter)...");
    match per_chapter_json(
        app,
        database,
        story_id,
        "ai_beta_reader",
        "ai_beta_reader_batch",
        "ai_beta_reader",
        provider,
        api_key,
        model,
        &bible,
        true,
    )
    .await
    {
        Ok(chapters) => {
            let avg_eng: f64 = chapters
                .iter()
                .filter_map(|c| {
                    c.get("engagement").and_then(|v| {
                        v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))
                    })
                })
                .sum::<f64>()
                / chapters.len().max(1) as f64;
            let avg_risk: f64 = chapters
                .iter()
                .filter_map(|c| {
                    c.get("put_down_risk").and_then(|v| {
                        v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))
                    })
                })
                .sum::<f64>()
                / chapters.len().max(1) as f64;
            let report = serde_json::json!({
                "schema": "ai_beta_reader_v1",
                "avg_engagement": avg_eng.round() as i64,
                "avg_put_down_risk": avg_risk.round() as i64,
                "chapters": chapters,
            })
            .to_string();
            let run_ts = chrono::Utc::now().to_rfc3339();
            let _ = db::save_document_at(
                &database.pool,
                story_id,
                "ai_beta_reader",
                &report,
                &run_ts,
            )
            .await;
            emit(
                app,
                &format!("✓ AI beta reader — {} chapter(s).", chapters.len()),
            );
            GenreResult {
                success: true,
                report,
                error: String::new(),
                run_ts,
            }
        }
        Err(e) => err(&e),
    }
}

pub async fn run_cliffhanger_score(
    app: &AppCtx,
    database: &db::Db,
    story_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
) -> GenreResult {
    if !crate::stories::story_exists(database, story_id).await {
        return err("Story not found.");
    }
    if api_key.is_empty() || model.is_empty() {
        return err("An API key and model are required. Set them in Settings.");
    }

    emit(app, "Scoring chapter endings...");
    match per_chapter_json(
        app,
        database,
        story_id,
        "cliffhanger_score",
        "cliffhanger_score_batch",
        "cliffhanger_score",
        provider,
        api_key,
        model,
        "",
        false,
    )
    .await
    {
        Ok(chapters) => {
            let avg: f64 = chapters
                .iter()
                .filter_map(|c| {
                    c.get("score").and_then(|v| {
                        v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))
                    })
                })
                .sum::<f64>()
                / chapters.len().max(1) as f64;
            let report = serde_json::json!({
                "schema": "cliffhanger_score_v1",
                "avg_score": avg.round() as i64,
                "chapters": chapters,
            })
            .to_string();
            let run_ts = chrono::Utc::now().to_rfc3339();
            let _ = db::save_document_at(
                &database.pool,
                story_id,
                "cliffhanger_score",
                &report,
                &run_ts,
            )
            .await;
            emit(
                app,
                &format!("✓ Cliffhanger scores — avg {}.", avg.round() as i64),
            );
            GenreResult {
                success: true,
                report,
                error: String::new(),
                run_ts,
            }
        }
        Err(e) => err(&e),
    }
}

pub async fn run_pacing_curve(
    app: &AppCtx,
    database: &db::Db,
    story_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
) -> GenreResult {
    if !crate::stories::story_exists(database, story_id).await {
        return err("Story not found.");
    }
    if api_key.is_empty() || model.is_empty() {
        return err("An API key and model are required. Set them in Settings.");
    }

    emit(app, "Building pacing curve...");
    match per_chapter_json(
        app,
        database,
        story_id,
        "pacing_curve",
        "pacing_curve_batch",
        "pacing_curve",
        provider,
        api_key,
        model,
        "",
        false,
    )
    .await
    {
        Ok(chapters) => {
            let report = serde_json::json!({
                "schema": "pacing_curve_v1",
                "chapters": chapters,
            })
            .to_string();
            let run_ts = chrono::Utc::now().to_rfc3339();
            let _ = db::save_document_at(
                &database.pool,
                story_id,
                "pacing_curve",
                &report,
                &run_ts,
            )
            .await;
            emit(
                app,
                &format!("✓ Pacing curve — {} chapter(s).", chapters.len()),
            );
            GenreResult {
                success: true,
                report,
                error: String::new(),
                run_ts,
            }
        }
        Err(e) => err(&e),
    }
}

pub async fn run_vellum_prep(app: &AppCtx, database: &db::Db, story_id: &str) -> GenreResult {
    if !crate::stories::story_exists(database, story_id).await {
        return err("Story not found.");
    }

    emit(app, "Preparing clean manuscript for Vellum / Atticus...");
    let chapters = match documents::list_chapters_db(database, story_id).await {
        Ok(c) => c,
        Err(e) => return err(&e),
    };
    if chapters.is_empty() {
        return err("No chapter documents found. Upload manuscript chapters first.");
    }

    let mut body = String::from("# Manuscript\n\n");
    let mut chapter_count = 0usize;
    for chapter in &chapters {
        let content = chapter.content.trim();
        if content.is_empty() {
            continue;
        }
        let cleaned = clean_for_formatter(content);
        if cleaned.trim().is_empty() {
            continue;
        }
        if chapter_count > 0 {
            body.push_str("\n\n\\page\n\n");
        }
        if !cleaned.lines().next().unwrap_or("").starts_with('#') {
            let title = if !chapter.title.is_empty() {
                chapter.title.clone()
            } else {
                extract_title(content).unwrap_or_else(|| format!("Chapter {}", chapter_count + 1))
            };
            body.push_str(&format!("# {}\n\n", title));
        }
        body.push_str(cleaned.trim());
        body.push('\n');
        chapter_count += 1;
    }

    let word_count = body.split_whitespace().count();
    let report = serde_json::json!({
        "schema": "vellum_prep_v1",
        "clean_markdown": body,
        "chapter_count": chapter_count,
        "word_count": word_count,
        "notes": [
            "Clean Markdown stored in this report for import into Vellum or Atticus.",
            "\\page markers separate chapters — replace with your formatter's page-break if needed.",
            "Copy or export this report's manuscript text into Vellum/Atticus (or convert to .docx)."
        ],
    })
    .to_string();
    let run_ts = chrono::Utc::now().to_rfc3339();
    let _ = db::save_document_at(&database.pool, story_id, "vellum_prep", &report, &run_ts).await;
    emit(
        app,
        &format!(
            "✓ Vellum prep — {} chapters, {} words.",
            chapter_count, word_count
        ),
    );
    GenreResult {
        success: true,
        report,
        error: String::new(),
        run_ts,
    }
}

fn clean_for_formatter(content: &str) -> String {
    let mut out = String::new();
    for line in content.lines() {
        let mut s = line.to_string();
        while let Some(start) = s.find('<') {
            if let Some(end) = s[start..].find('>') {
                s.replace_range(start..start + end + 1, "");
            } else {
                break;
            }
        }
        let trimmed = s.trim_end();
        out.push_str(trimmed);
        out.push('\n');
    }
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out
}

pub async fn run_hook_strength(
    app: &AppCtx,
    database: &db::Db,
    story_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible_path: &str,
) -> GenreResult {
    if !crate::stories::story_exists(database, story_id).await {
        return err("Story not found.");
    }
    if api_key.is_empty() || model.is_empty() {
        return err("An API key and model are required. Set them in Settings.");
    }

    emit(app, "Evaluating opening hook...");
    let manuscript = match build_opening_excerpt(database, story_id).await {
        Ok(m) => m,
        Err(e) => return err(&e),
    };
    let bible = prompts::load_bible_for_story(&app.db, story_id, bible_path).await;
    let mut vars = HashMap::new();
    vars.insert("bible", bible.as_str());
    vars.insert("manuscript", manuscript.as_str());

    let run_ts = chrono::Utc::now().to_rfc3339();
    match prompts::execute_prompt(
        app,
        "hook_strength",
        provider,
        api_key,
        model,
        vars,
        Some(story_id),
    )
    .await
    {
        Ok(raw) => {
            let clean = match extract_json_object(&raw) {
                Some(c) => c,
                None => return err("No JSON in hook strength response."),
            };
            let parsed: serde_json::Value = match serde_json::from_str(&clean) {
                Ok(v) => v,
                Err(e) => return err(&format!("JSON parse: {}", e)),
            };
            let report = serde_json::json!({
                "schema": "hook_strength_v1",
                "score": parsed.get("score").cloned().unwrap_or(serde_json::json!(0)),
                "verdict": parsed.get("verdict").and_then(|v| v.as_str()).unwrap_or(""),
                "summary": parsed.get("summary").and_then(|v| v.as_str()).unwrap_or(""),
                "strengths": parsed.get("strengths").cloned().unwrap_or(serde_json::json!([])),
                "weaknesses": parsed.get("weaknesses").cloned().unwrap_or(serde_json::json!([])),
                "first_friction_point": parsed.get("first_friction_point").and_then(|v| v.as_str()).unwrap_or(""),
            })
            .to_string();
            let _ = db::save_document_at(&database.pool, story_id, "hook_strength", &report, &run_ts).await;
            emit(app, "✓ Hook strength complete.");
            GenreResult {
                success: true,
                report,
                error: String::new(),
                run_ts,
            }
        }
        Err(e) => err(&e),
    }
}

// ── Line-level polish (heuristic, no AI) ─────────────────────────────────────

const FILTER_WORDS: &[&str] = &[
    "just", "really", "very", "quite", "rather", "somehow", "suddenly", "actually",
    "basically", "literally", "definitely", "probably", "maybe", "perhaps",
    "somewhat", "almost", "nearly", "simply", "merely",
];

pub async fn run_line_polish(app: &AppCtx, database: &db::Db, story_id: &str) -> GenreResult {
    if !crate::stories::story_exists(database, story_id).await {
        return err("Story not found.");
    }

    emit(app, "Running line-level polish (heuristic — no AI)...");
    let chapters = match documents::list_chapters_db(database, story_id).await {
        Ok(c) => c,
        Err(e) => return err(&e),
    };
    if chapters.is_empty() {
        return err("No chapter documents found. Upload manuscript chapters first.");
    }

    let mut chapter_rows = Vec::new();
    let mut totals = serde_json::json!({
        "filter_words": 0, "echoes": 0, "adverbs": 0, "passive": 0
    });

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
        let hits = polish_scan(content);
        for key in ["filter_words", "echoes", "adverbs", "passive"] {
            let n = hits
                .get(key)
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0) as i64;
            if let Some(t) = totals.get_mut(key) {
                *t = serde_json::json!(t.as_i64().unwrap_or(0) + n);
            }
        }
        chapter_rows.push(serde_json::json!({
            "file": filename,
            "title": title,
            "chapter_index": i,
            "hits": hits,
        }));
    }

    let report = serde_json::json!({
        "schema": "line_polish_v1",
        "totals": totals,
        "chapters": chapter_rows,
    })
    .to_string();
    let run_ts = chrono::Utc::now().to_rfc3339();
    let _ = db::save_document_at(&database.pool, story_id, "line_polish", &report, &run_ts).await;
    emit(app, "✓ Line-level polish complete.");
    GenreResult {
        success: true,
        report,
        error: String::new(),
        run_ts,
    }
}

fn polish_scan(content: &str) -> serde_json::Value {
    let lower = content.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();

    let mut filter_hits = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let clean = w.trim_matches(|c: char| !c.is_alphabetic());
        if FILTER_WORDS.contains(&clean) {
            filter_hits.push(serde_json::json!({
                "word": clean,
                "index": i,
                "context": context_window(&words, i, 4),
            }));
            if filter_hits.len() >= 40 {
                break;
            }
        }
    }

    let mut echo_hits = Vec::new();
    for i in 0..words.len() {
        let a = words[i].trim_matches(|c: char| !c.is_alphabetic());
        if a.len() < 4 {
            continue;
        }
        for j in (i + 1)..(i + 8).min(words.len()) {
            let b = words[j].trim_matches(|c: char| !c.is_alphabetic());
            if a == b {
                echo_hits.push(serde_json::json!({
                    "word": a,
                    "index": i,
                    "context": context_window(&words, i, 5),
                }));
                break;
            }
        }
        if echo_hits.len() >= 30 {
            break;
        }
    }

    let mut adverb_hits = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let clean = w.trim_matches(|c: char| !c.is_alphabetic());
        if clean.len() > 5
            && clean.ends_with("ly")
            && !matches!(
                clean,
                "only" | "family" | "early" | "really" | "supply" | "apply"
            )
        {
            adverb_hits.push(serde_json::json!({
                "word": clean,
                "index": i,
                "context": context_window(&words, i, 4),
            }));
            if adverb_hits.len() >= 30 {
                break;
            }
        }
    }

    let mut passive_hits = Vec::new();
    for i in 0..words.len().saturating_sub(1) {
        let aux = words[i].trim_matches(|c: char| !c.is_alphabetic());
        let next = words[i + 1].trim_matches(|c: char| !c.is_alphabetic());
        if matches!(aux, "was" | "were" | "been" | "being" | "is" | "are" | "be")
            && (next.ends_with("ed") || next.ends_with("en"))
        {
            passive_hits.push(serde_json::json!({
                "phrase": format!("{} {}", aux, next),
                "index": i,
                "context": context_window(&words, i, 5),
            }));
            if passive_hits.len() >= 30 {
                break;
            }
        }
    }

    serde_json::json!({
        "filter_words": filter_hits,
        "echoes": echo_hits,
        "adverbs": adverb_hits,
        "passive": passive_hits,
    })
}

fn context_window(words: &[&str], i: usize, radius: usize) -> String {
    let start = i.saturating_sub(radius);
    let end = (i + radius + 1).min(words.len());
    words[start..end].join(" ")
}

// ── Blurb Builder ─────────────────────────────────────────────────────────────

pub async fn run_blurb_builder(
    app: &AppCtx,
    database: &db::Db,
    story_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
) -> GenreResult {
    if !crate::stories::story_exists(database, story_id).await {
        return err("Story not found.");
    }
    if api_key.is_empty() || model.is_empty() {
        return err("An API key and model are required. Set them in Settings.");
    }

    emit(app, "Building book description variants...");
    let summaries = db::load_chapter_summaries(&database.pool, story_id).await;
    if summaries.is_empty() {
        return err("Chapter summaries required. Refresh summaries in Settings → Story Data.");
    }

    let combined = build_combined_context(&summaries);
    let genre_context = db::load_genre_data(&database.pool, story_id)
        .await
        .map(|g| {
            format!(
                "Ebook: {}\nPrint: {}\nSignals: {}\nDemographic: {}",
                g.industry_ebook, g.industry_print, g.genre_signals, g.reader_demographic
            )
        })
        .unwrap_or_default();

    let mut vars = HashMap::new();
    vars.insert("combined", combined.as_str());
    vars.insert("genre_context", genre_context.as_str());

    let run_ts = chrono::Utc::now().to_rfc3339();
    let raw = match prompts::execute_prompt(
        app,
        "blurb_builder",
        provider,
        api_key,
        model,
        vars,
        Some(story_id),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return err(&format!("Blurb Builder AI error: {}", e)),
    };

    let clean = match extract_json_object(&raw) {
        Some(c) => c,
        None => {
            return err(&format!(
                "No JSON in blurb response: {}",
                &raw[..raw.len().min(200)]
            ));
        }
    };
    let mut value: serde_json::Value = match serde_json::from_str(&clean) {
        Ok(v) => v,
        Err(e) => return err(&format!("Blurb JSON parse error: {}", e)),
    };
    if let Some(obj) = value.as_object_mut() {
        obj.insert("schema".into(), serde_json::json!("blurb_builder_v1"));
    }
    let report = value.to_string();
    let _ = db::save_document_at(&database.pool, story_id, "blurb_builder", &report, &run_ts).await;
    emit(app, "✓ Blurb Builder saved.");
    GenreResult {
        success: true,
        report,
        error: String::new(),
        run_ts,
    }
}

// ── Print Production ──────────────────────────────────────────────────────────

pub async fn run_print_production(app: &AppCtx, database: &db::Db, story_id: &str) -> GenreResult {
    if !crate::stories::story_exists(database, story_id).await {
        return err("Story not found.");
    }

    emit(app, "Calculating print production specs...");
    let chapters = match documents::list_chapters_db(database, story_id).await {
        Ok(c) => c,
        Err(e) => return err(&e),
    };
    if chapters.is_empty() {
        return err("No chapter documents found. Upload manuscript chapters first.");
    }

    let mut word_count = 0usize;
    for chapter in &chapters {
        word_count += chapter.content.split_whitespace().count();
    }

    let pages = ((word_count as f64) / 250.0).ceil() as u32;
    let pages = pages.max(24);

    let (trim, trim_reason) = if word_count < 50_000 {
        (
            "5 x 8 in",
            "Shorter fiction — common trade size for under ~50k words.",
        )
    } else if word_count < 100_000 {
        (
            "6 x 9 in",
            "Standard trade paperback for mid-length fiction.",
        )
    } else {
        (
            "6 x 9 in",
            "Longer fiction — 6×9 keeps page count manageable.",
        )
    };

    let spine_inches = (pages as f64 / 444.0) + 0.06;
    let paper = if word_count > 90_000 { "cream" } else { "white" };

    let genre_label = db::load_genre_data(&database.pool, story_id)
        .await
        .map(|g| g.industry_print.clone())
        .unwrap_or_default();

    let report = serde_json::json!({
        "schema": "print_production_v1",
        "word_count": word_count,
        "estimated_pages": pages,
        "trim_size": trim,
        "trim_rationale": trim_reason,
        "spine_inches": format!("{:.3}", spine_inches),
        "paper_color": paper,
        "ink": "black",
        "genre_label_print": genre_label,
        "ingram_checklist": [
            "Verify BISAC codes (bisg.org) before upload",
            "Set trim size and page count to match final PDF",
            "Upload print-ready PDF with embedded fonts",
            "Set wholesale discount (typically 40–55% for libraries)",
            "Confirm laminate (matte/gloss) and color profile",
            "Add description and author bio matching wide listing copy",
        ],
        "kdp_print_checklist": [
            "Paperback uses same 7 keywords as Kindle listing",
            "Select paperback browse categories (separate from Kindle)",
            "Enter BISAC subject codes on print details page",
            "Upload cover PDF with correct spine width",
        ],
    })
    .to_string();

    let run_ts = chrono::Utc::now().to_rfc3339();
    let _ = db::save_document_at(&database.pool, story_id, "print_production", &report, &run_ts).await;
    emit(
        app,
        &format!("✓ Print production specs saved (~{} pages, {}).", pages, trim),
    );
    GenreResult {
        success: true,
        report,
        error: String::new(),
        run_ts,
    }
}
