// analysis/publish_audits.rs — Publish + Marketing platform analyses.
// Publish: reader experience / polish. Marketing: blurbs and ad copy.

use std::collections::HashMap;
use std::path::PathBuf;
use tauri::AppHandle;

use super::chapters::{build_combined_context, collect_chapters, extract_title};
use super::craft_audits::build_opening_excerpt;
use super::{emit, err, extract_json_object, GenreResult};
use crate::db;
use crate::prompts;

fn truncate_words(text: &str, max: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max {
        return text.to_string();
    }
    words[..max].join(" ") + "\n\n[Truncated]"
}

async fn per_chapter_json(
    app: &AppHandle,
    database: &db::Db,
    folder: &str,
    template_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible: &str,
    include_bible: bool,
) -> Result<Vec<serde_json::Value>, String> {
    let path = PathBuf::from(folder);
    let chapters = collect_chapters(&path);
    if chapters.is_empty() {
        return Err("No .md chapter files found.".into());
    }

    let mut rows = Vec::new();
    for (i, ch_path) in chapters.iter().enumerate() {
        if crate::is_cancelled() {
            return Err("Cancelled.".into());
        }
        let content = match std::fs::read_to_string(ch_path) {
            Ok(c) if !c.trim().is_empty() => c,
            _ => continue,
        };
        let filename = ch_path
            .strip_prefix(&path)
            .unwrap_or(ch_path)
            .to_string_lossy()
            .to_string();
        let title = extract_title(&content).unwrap_or_else(|| filename.clone());
        let chapter_text = truncate_words(&content, 4000);

        emit(app, &format!("  [{}/{}] {}...", i + 1, chapters.len(), filename));

        let mut vars = HashMap::new();
        vars.insert("chapter_title", title.as_str());
        vars.insert("chapter_text", chapter_text.as_str());
        let empty = "";
        vars.insert("bible", if include_bible { bible } else { empty });

        let raw = prompts::execute_prompt(database, template_id, provider, api_key, model, vars).await?;
        let clean = extract_json_object(&raw)
            .ok_or_else(|| format!("No JSON for {}", filename))?;
        let mut parsed: serde_json::Value = serde_json::from_str(&clean)
            .map_err(|e| format!("Parse {}: {}", filename, e))?;
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
    app: &AppHandle,
    database: &db::Db,
    folder: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible_path: &str,
) -> GenreResult {
    if api_key.is_empty() || model.is_empty() {
        return err("AI Beta Reader requires an API key and model.");
    }
    let bible = prompts::load_bible_for_story(folder, bible_path);
    emit(app, "Running AI beta reader (per chapter)...");
    match per_chapter_json(app, database, folder, "ai_beta_reader", provider, api_key, model, &bible, true).await {
        Ok(chapters) => {
            let avg_eng: f64 = chapters.iter()
                .filter_map(|c| c.get("engagement").and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))))
                .sum::<f64>() / chapters.len().max(1) as f64;
            let avg_risk: f64 = chapters.iter()
                .filter_map(|c| c.get("put_down_risk").and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))))
                .sum::<f64>() / chapters.len().max(1) as f64;
            let report = serde_json::json!({
                "schema": "ai_beta_reader_v1",
                "avg_engagement": avg_eng.round() as i64,
                "avg_put_down_risk": avg_risk.round() as i64,
                "chapters": chapters,
            }).to_string();
            let conn = database.0.lock().unwrap();
            let _ = db::save_document(&conn, folder, "ai_beta_reader", &report);
            emit(app, &format!("✓ AI beta reader — {} chapter(s).", chapters.len()));
            GenreResult { success: true, report, error: String::new(), run_ts: chrono::Utc::now().to_rfc3339() }
        }
        Err(e) => err(&e),
    }
}

pub async fn run_cliffhanger_score(
    app: &AppHandle,
    database: &db::Db,
    folder: &str,
    provider: &str,
    api_key: &str,
    model: &str,
) -> GenreResult {
    if api_key.is_empty() || model.is_empty() {
        return err("Cliffhanger Score requires an API key and model.");
    }
    emit(app, "Scoring chapter endings...");
    match per_chapter_json(app, database, folder, "cliffhanger_score", provider, api_key, model, "", false).await {
        Ok(chapters) => {
            let avg: f64 = chapters.iter()
                .filter_map(|c| c.get("score").and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))))
                .sum::<f64>() / chapters.len().max(1) as f64;
            let report = serde_json::json!({
                "schema": "cliffhanger_score_v1",
                "avg_score": avg.round() as i64,
                "chapters": chapters,
            }).to_string();
            let conn = database.0.lock().unwrap();
            let _ = db::save_document(&conn, folder, "cliffhanger_score", &report);
            emit(app, &format!("✓ Cliffhanger scores — avg {}.", avg.round() as i64));
            GenreResult { success: true, report, error: String::new(), run_ts: chrono::Utc::now().to_rfc3339() }
        }
        Err(e) => err(&e),
    }
}

pub async fn run_hook_strength(
    app: &AppHandle,
    database: &db::Db,
    folder: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible_path: &str,
) -> GenreResult {
    if api_key.is_empty() || model.is_empty() {
        return err("Hook Strength requires an API key and model.");
    }
    emit(app, "Evaluating opening hook...");
    let manuscript = match build_opening_excerpt(folder) {
        Ok(m) => m,
        Err(e) => return err(&e),
    };
    let bible = prompts::load_bible_for_story(folder, bible_path);
    let mut vars = HashMap::new();
    vars.insert("bible", bible.as_str());
    vars.insert("manuscript", manuscript.as_str());
    match prompts::execute_prompt(database, "hook_strength", provider, api_key, model, vars).await {
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
            }).to_string();
            let conn = database.0.lock().unwrap();
            let _ = db::save_document(&conn, folder, "hook_strength", &report);
            emit(app, "✓ Hook strength complete.");
            GenreResult { success: true, report, error: String::new(), run_ts: chrono::Utc::now().to_rfc3339() }
        }
        Err(e) => err(&e),
    }
}

pub async fn run_pacing_curve(
    app: &AppHandle,
    database: &db::Db,
    folder: &str,
    provider: &str,
    api_key: &str,
    model: &str,
) -> GenreResult {
    if api_key.is_empty() || model.is_empty() {
        return err("Pacing Curve requires an API key and model.");
    }
    emit(app, "Building pacing curve...");
    match per_chapter_json(app, database, folder, "pacing_curve", provider, api_key, model, "", false).await {
        Ok(chapters) => {
            let report = serde_json::json!({
                "schema": "pacing_curve_v1",
                "chapters": chapters,
            }).to_string();
            let conn = database.0.lock().unwrap();
            let _ = db::save_document(&conn, folder, "pacing_curve", &report);
            emit(app, &format!("✓ Pacing curve — {} chapter(s).", chapters.len()));
            GenreResult { success: true, report, error: String::new(), run_ts: chrono::Utc::now().to_rfc3339() }
        }
        Err(e) => err(&e),
    }
}

fn genre_context_from_row(g: &db::GenreDataRow) -> String {
    format!(
        "Industry ebook: {}\nIndustry print: {}\nGenre signals: {}\nReader demographic: {}\nBookstore shelving: {}\nComps ebook: {}\nComps print: {}\nMarketing notes: {}",
        g.industry_ebook,
        g.industry_print,
        g.genre_signals,
        g.reader_demographic,
        g.bookstore_shelving,
        g.comps_ebook.join("; "),
        g.comps_print.join("; "),
        g.marketing_notes.join("; "),
    )
}

pub async fn run_blurb_builder(
    app: &AppHandle,
    database: &db::Db,
    folder: &str,
    provider: &str,
    api_key: &str,
    model: &str,
) -> GenreResult {
    if api_key.is_empty() || model.is_empty() {
        return err("Blurb Builder requires an API key and model.");
    }

    let (summaries, genre_context) = {
        let conn = database.0.lock().unwrap();
        let summaries = db::load_chapter_summaries(&conn, folder);
        let genre_context = db::load_genre_data(&conn, folder)
            .map(|g| genre_context_from_row(&g))
            .unwrap_or_default();
        (summaries, genre_context)
    };

    if summaries.is_empty() {
        return err("Blurb Builder needs Chapter Summaries first. Run them from the KDP or Wide Analyzer tab, then try again.");
    }

    emit(app, "Building blurb variants...");
    let combined = build_combined_context(&summaries);
    let empty_genre = "(No genre analysis on file — write from chapter summaries only.)";
    let genre_ctx = if genre_context.trim().is_empty() {
        empty_genre
    } else {
        genre_context.as_str()
    };

    let mut vars = HashMap::new();
    vars.insert("combined", combined.as_str());
    vars.insert("genre_context", genre_ctx);

    match prompts::execute_prompt(database, "blurb_builder", provider, api_key, model, vars).await {
        Ok(raw) => {
            let clean = match extract_json_object(&raw) {
                Some(c) => c,
                None => return err("No JSON in blurb builder response."),
            };
            let parsed: serde_json::Value = match serde_json::from_str(&clean) {
                Ok(v) => v,
                Err(e) => return err(&format!("JSON parse: {}", e)),
            };

            let variants = parsed.get("variants").cloned().unwrap_or(serde_json::json!([]));
            let report = serde_json::json!({
                "schema": "blurb_builder_v1",
                "variants": variants,
                "short_form": parsed.get("short_form").and_then(|v| v.as_str()).unwrap_or(""),
                "bookbub_one_liner": parsed.get("bookbub_one_liner").and_then(|v| v.as_str()).unwrap_or(""),
                "used_genre_analysis": !genre_context.trim().is_empty(),
            }).to_string();

            let conn = database.0.lock().unwrap();
            let _ = db::save_document(&conn, folder, "blurb_builder", &report);
            let n = variants.as_array().map(|a| a.len()).unwrap_or(0);
            emit(app, &format!("✓ Blurb Builder — {} variant(s).", n));
            GenreResult { success: true, report, error: String::new(), run_ts: chrono::Utc::now().to_rfc3339() }
        }
        Err(e) => err(&e),
    }
}

fn blurb_context_from_doc(raw: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return raw.chars().take(4000).collect(),
    };
    let mut parts = Vec::new();
    if let Some(variants) = parsed.get("variants").and_then(|v| v.as_array()) {
        for (i, v) in variants.iter().enumerate() {
            let angle = v.get("angle").and_then(|a| a.as_str()).unwrap_or("variant");
            let blurb = v.get("blurb").and_then(|a| a.as_str()).unwrap_or("");
            if !blurb.is_empty() {
                parts.push(format!("Variant {} ({}):\n{}", i + 1, angle, blurb));
            }
        }
    }
    if let Some(s) = parsed.get("short_form").and_then(|v| v.as_str()) {
        if !s.is_empty() {
            parts.push(format!("Short-form: {}", s));
        }
    }
    if let Some(s) = parsed.get("bookbub_one_liner").and_then(|v| v.as_str()) {
        if !s.is_empty() {
            parts.push(format!("BookBub one-liner: {}", s));
        }
    }
    if parts.is_empty() {
        raw.chars().take(4000).collect()
    } else {
        parts.join("\n\n")
    }
}

pub async fn run_ad_copy_hook_pack(
    app: &AppHandle,
    database: &db::Db,
    folder: &str,
    provider: &str,
    api_key: &str,
    model: &str,
) -> GenreResult {
    if api_key.is_empty() || model.is_empty() {
        return err("Ad Copy & Hook Pack requires an API key and model.");
    }

    let (summaries, genre_context, blurb_raw, search_terms) = {
        let conn = database.0.lock().unwrap();
        let summaries = db::load_chapter_summaries(&conn, folder);
        let genre_context = db::load_genre_data(&conn, folder)
            .map(|g| genre_context_from_row(&g))
            .unwrap_or_default();
        let blurb_raw = db::get_document(&conn, folder, "blurb_builder");
        let search_terms = db::load_mi_search_terms(&conn, folder);
        (summaries, genre_context, blurb_raw, search_terms)
    };

    if summaries.is_empty() {
        return err("Ad Copy & Hook Pack needs Chapter Summaries first. Run them from the KDP or Wide Analyzer tab, then try again.");
    }

    emit(app, "Building Ad Copy & Hook Pack...");
    let combined = build_combined_context(&summaries);
    let empty_genre = "(No genre analysis on file — write from chapter summaries only.)";
    let genre_ctx = if genre_context.trim().is_empty() {
        empty_genre
    } else {
        genre_context.as_str()
    };

    let used_blurb = blurb_raw.is_some();
    let blurb_formatted = blurb_raw.as_deref().map(blurb_context_from_doc);
    let empty_blurb = "(No Blurb Builder output on file — generate ad copy independently from summaries and genre context.)";
    let blurb_ctx = blurb_formatted.as_deref().unwrap_or(empty_blurb);

    let avoid_terms = if search_terms.is_empty() {
        "(No Search Terms report on file.)".to_string()
    } else {
        search_terms.join(", ")
    };

    let mut vars = HashMap::new();
    vars.insert("combined", combined.as_str());
    vars.insert("genre_context", genre_ctx);
    vars.insert("blurb_context", blurb_ctx);
    vars.insert("avoid_search_terms", avoid_terms.as_str());

    match prompts::execute_prompt(database, "ad_copy_hook_pack", provider, api_key, model, vars).await {
        Ok(raw) => {
            let clean = match extract_json_object(&raw) {
                Some(c) => c,
                None => return err("No JSON in ad copy response."),
            };
            let parsed: serde_json::Value = match serde_json::from_str(&clean) {
                Ok(v) => v,
                Err(e) => return err(&format!("JSON parse: {}", e)),
            };

            let report = serde_json::json!({
                "schema": "ad_copy_hook_pack_v1",
                "disclaimer": "AI-reasoned creative copy for you to test and edit. Not guaranteed-performing ad copy — no CTR, conversion, or performance claims implied.",
                "bookbub_headlines": parsed.get("bookbub_headlines").cloned().unwrap_or(serde_json::json!([])),
                "ad_hooks": parsed.get("ad_hooks").cloned().unwrap_or(serde_json::json!([])),
                "ams_seed_phrases": parsed.get("ams_seed_phrases").cloned().unwrap_or(serde_json::json!([])),
                "used_genre_analysis": !genre_context.trim().is_empty(),
                "used_blurb_builder": used_blurb,
            }).to_string();

            let conn = database.0.lock().unwrap();
            let _ = db::save_document(&conn, folder, "ad_copy_hook_pack", &report);
            emit(app, "✓ Ad Copy & Hook Pack complete.");
            GenreResult { success: true, report, error: String::new(), run_ts: chrono::Utc::now().to_rfc3339() }
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

pub fn run_line_polish(app: &AppHandle, database: &db::Db, folder: &str) -> GenreResult {
    emit(app, "Running line-level polish (heuristic — no AI)...");
    let path = PathBuf::from(folder);
    let chapters = collect_chapters(&path);
    if chapters.is_empty() {
        return err("No .md chapter files found.");
    }

    let mut chapter_rows = Vec::new();
    let mut totals = serde_json::json!({
        "filter_words": 0, "echoes": 0, "adverbs": 0, "passive": 0
    });

    for (i, ch_path) in chapters.iter().enumerate() {
        let content = match std::fs::read_to_string(ch_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let filename = ch_path.strip_prefix(&path).unwrap_or(ch_path).to_string_lossy().to_string();
        let title = extract_title(&content).unwrap_or_else(|| filename.clone());
        let hits = polish_scan(&content);
        for key in ["filter_words", "echoes", "adverbs", "passive"] {
            let n = hits.get(key).and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0) as i64;
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
    }).to_string();
    let conn = database.0.lock().unwrap();
    let _ = db::save_document(&conn, folder, "line_polish", &report);
    emit(app, "✓ Line-level polish complete.");
    GenreResult { success: true, report, error: String::new(), run_ts: chrono::Utc::now().to_rfc3339() }
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
            if filter_hits.len() >= 40 { break; }
        }
    }

    let mut echo_hits = Vec::new();
    for i in 0..words.len() {
        let a = words[i].trim_matches(|c: char| !c.is_alphabetic());
        if a.len() < 4 { continue; }
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
        if echo_hits.len() >= 30 { break; }
    }

    let mut adverb_hits = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let clean = w.trim_matches(|c: char| !c.is_alphabetic());
        if clean.len() > 5 && clean.ends_with("ly")
            && !matches!(clean, "only" | "family" | "early" | "really" | "supply" | "apply")
        {
            adverb_hits.push(serde_json::json!({
                "word": clean,
                "index": i,
                "context": context_window(&words, i, 4),
            }));
            if adverb_hits.len() >= 30 { break; }
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
            if passive_hits.len() >= 30 { break; }
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

// ── Vellum / Atticus prep ─────────────────────────────────────────────────────

pub fn run_vellum_prep(app: &AppHandle, database: &db::Db, folder: &str) -> GenreResult {
    emit(app, "Preparing clean manuscript for Vellum / Atticus...");
    let root = PathBuf::from(folder);
    let chapters = collect_chapters(&root);
    if chapters.is_empty() {
        return err("No .md chapter files found.");
    }

    let mut body = String::from("# Manuscript\n\n");
    let mut chapter_count = 0usize;
    for path in &chapters {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let cleaned = clean_for_formatter(&content);
        if cleaned.trim().is_empty() {
            continue;
        }
        if chapter_count > 0 {
            body.push_str("\n\n\\page\n\n");
        }
        // Ensure a chapter heading
        if !cleaned.lines().next().unwrap_or("").starts_with('#') {
            let title = extract_title(&content)
                .unwrap_or_else(|| format!("Chapter {}", chapter_count + 1));
            body.push_str(&format!("# {}\n\n", title));
        }
        body.push_str(cleaned.trim());
        body.push('\n');
        chapter_count += 1;
    }

    let out_dir = root.join("Publishing");
    let _ = std::fs::create_dir_all(&out_dir);
    let out_path = out_dir.join("manuscript-clean.md");
    if let Err(e) = std::fs::write(&out_path, &body) {
        return err(&format!("Could not write {}: {}", out_path.display(), e));
    }

    let report = serde_json::json!({
        "schema": "vellum_prep_v1",
        "output_path": out_path.to_string_lossy(),
        "chapter_count": chapter_count,
        "word_count": body.split_whitespace().count(),
        "notes": [
            "Clean Markdown written for import into Vellum or Atticus.",
            "\\page markers separate chapters — replace with your formatter's page-break if needed.",
            "Import manuscript-clean.md (or convert to .docx) into Vellum/Atticus."
        ],
    }).to_string();
    let conn = database.0.lock().unwrap();
    let _ = db::save_document(&conn, folder, "vellum_prep", &report);
    emit(app, &format!("✓ Wrote {}", out_path.display()));
    GenreResult { success: true, report, error: String::new(), run_ts: chrono::Utc::now().to_rfc3339() }
}

fn clean_for_formatter(content: &str) -> String {
    let mut out = String::new();
    for line in content.lines() {
        // Strip HTML-ish tags lightly
        let mut s = line.to_string();
        while let Some(start) = s.find('<') {
            if let Some(end) = s[start..].find('>') {
                s.replace_range(start..start + end + 1, "");
            } else {
                break;
            }
        }
        // Collapse weird spaces
        let trimmed = s.trim_end();
        out.push_str(trimmed);
        out.push('\n');
    }
    // Collapse 3+ blank lines to 2
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out
}
