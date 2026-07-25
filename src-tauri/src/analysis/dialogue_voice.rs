// analysis/dialogue_voice.rs — Dialogue & Voice (Craft):
//   Pass 1 (non-AI): dialogue-to-narrative ratio per chapter + overall.
//   Pass 2 (AI): voice-similarity judgment across major characters.
// Story Bible / Characters content is required (no silent fallback).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

use super::chapters::{collect_chapters, extract_title};
use super::{emit, err, extract_json_object, GenreResult};
use crate::db;
use crate::prompts;

const RATIO_REF_LOW: f64 = 0.25;
const RATIO_REF_HIGH: f64 = 0.35;
const MAX_SAMPLES_PER_CHAR: usize = 8;
const MAX_SAMPLE_CHARS: usize = 280;

/// Run Dialogue & Voice for a story folder (used by craft pipeline).
pub async fn run_dialogue_voice(
    app: &AppHandle,
    database: &db::Db,
    folder: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible_path: &str,
) -> GenreResult {
    if api_key.is_empty() || model.is_empty() {
        return err("Dialogue & Voice requires an API key and model. Set them in Settings.");
    }

    let path = PathBuf::from(folder);
    if !path.exists() {
        return err("Folder does not exist.");
    }

    let bible = prompts::load_bible_for_story(folder, bible_path);
    if bible.trim().is_empty() {
        return err(
            "Dialogue & Voice requires Story Bible and/or Characters content. \
             Add files under your configured Bible or Characters folders (or set a bible path on the story), then try again.",
        );
    }

    let characters = discover_character_names(folder, &bible);
    if characters.len() < 2 {
        return err(
            "Dialogue & Voice needs at least two character names from the Story Bible / Characters folder \
             (filename stems or # headings). Add character sheets, then try again.",
        );
    }

    let chapters = collect_chapters(&path);
    if chapters.is_empty() {
        return err("No .md chapter files found.");
    }

    emit(app, "Dialogue & Voice: computing dialogue ratios...");
    let mut chapter_ratios = Vec::new();
    let mut total_dialogue = 0usize;
    let mut total_words = 0usize;
    let mut attributed: HashMap<String, Vec<String>> = HashMap::new();
    for name in &characters {
        attributed.insert(name.clone(), Vec::new());
    }

    for (i, ch_path) in chapters.iter().enumerate() {
        if crate::is_cancelled() {
            return err("Cancelled.");
        }
        let content = match std::fs::read_to_string(ch_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let filename = ch_path
            .strip_prefix(&path)
            .unwrap_or(ch_path)
            .to_string_lossy()
            .to_string();
        let title = extract_title(&content).unwrap_or_else(|| filename.clone());
        let (dialogue_words, narrative_words, quotes) = measure_dialogue(&content);
        let words = dialogue_words + narrative_words;
        let ratio = if words == 0 {
            0.0
        } else {
            dialogue_words as f64 / words as f64
        };
        total_dialogue += dialogue_words;
        total_words += words;

        let outside_ref = ratio < RATIO_REF_LOW || ratio > RATIO_REF_HIGH;
        let flag = if words == 0 {
            None
        } else if ratio < RATIO_REF_LOW {
            Some("below_reference_range")
        } else if ratio > RATIO_REF_HIGH {
            Some("above_reference_range")
        } else {
            None
        };

        chapter_ratios.push(serde_json::json!({
            "file": filename,
            "title": title,
            "chapter_index": i,
            "dialogue_words": dialogue_words,
            "narrative_words": narrative_words,
            "total_words": words,
            "dialogue_ratio": (ratio * 1000.0).round() / 10.0,
            "outside_reference_range": outside_ref,
            "flag": flag,
        }));

        // Attribute quoted lines near known character names
        attribute_quotes(&characters, &quotes, &content, &mut attributed);
    }

    let overall_ratio = if total_words == 0 {
        0.0
    } else {
        total_dialogue as f64 / total_words as f64
    };

    // Build sample pack for AI (only characters with dialogue)
    let mut sample_blocks: Vec<String> = Vec::new();
    let mut sampled_names: Vec<String> = Vec::new();
    for name in &characters {
        let samples = attributed.get(name).cloned().unwrap_or_default();
        if samples.is_empty() {
            continue;
        }
        sampled_names.push(name.clone());
        let joined = samples
            .iter()
            .take(MAX_SAMPLES_PER_CHAR)
            .map(|s| format!("  - \"{}\"", s))
            .collect::<Vec<_>>()
            .join("\n");
        sample_blocks.push(format!("{}:\n{}", name, joined));
    }

    if sampled_names.len() < 2 {
        return err(
            "Could not attribute dialogue quotes to at least two named characters. \
             Ensure major characters appear in dialogue tags or near their lines in the manuscript.",
        );
    }

    emit(app, "Dialogue & Voice: checking voice distinction (AI)...");
    let samples_text = sample_blocks.join("\n\n");
    let char_list = sampled_names.join(", ");
    let mut vars = HashMap::new();
    vars.insert("bible", bible.as_str());
    vars.insert("characters", char_list.as_str());
    vars.insert("dialogue_samples", samples_text.as_str());

    let flags = match prompts::execute_prompt(database, "dialogue_voice", provider, api_key, model, vars).await {
        Ok(raw) => match parse_voice_flags(&raw) {
            Ok(f) => f,
            Err(e) => return err(&e),
        },
        Err(e) => return err(&e),
    };

    let report = serde_json::json!({
        "schema": "dialogue_voice_v1",
        "ratio_note": "Dialogue ratio uses a general 25–35% reference range for genre fiction — a reference point, not a rigid rule. Dialogue-heavy or sparse chapters can be intentional.",
        "voice_note": "AI-assisted: voice-similarity flags are judgment prompts to revisit, not a verdict that characters must sound different in every line.",
        "overall": {
            "dialogue_words": total_dialogue,
            "narrative_words": total_words.saturating_sub(total_dialogue),
            "total_words": total_words,
            "dialogue_ratio": (overall_ratio * 1000.0).round() / 10.0,
            "outside_reference_range": overall_ratio < RATIO_REF_LOW || overall_ratio > RATIO_REF_HIGH,
            "reference_range": "25–35%",
        },
        "chapters": chapter_ratios,
        "characters_used": sampled_names,
        "voice_flags": flags,
    })
    .to_string();

    {
        let conn = database.0.lock().unwrap();
        let _ = db::save_document(&conn, folder, "dialogue_voice", &report);
    }
    emit(
        app,
        &format!(
            "✓ Dialogue & Voice — overall dialogue {:.1}% · {} voice flag(s).",
            overall_ratio * 100.0,
            flags.len()
        ),
    );
    GenreResult {
        success: true,
        report,
        error: String::new(),
        run_ts: chrono::Utc::now().to_rfc3339(),
    }
}

// ── Character discovery ──────────────────────────────────────────────────────

fn discover_character_names(folder: &str, bible: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let push = |names: &mut Vec<String>, raw: &str| {
        let n = clean_name(raw);
        if n.len() < 2 {
            return;
        }
        if !names.iter().any(|e| e.eq_ignore_ascii_case(&n)) {
            names.push(n);
        }
    };

    let root = Path::new(folder);
    let structure = crate::folder_structure::current();
    if let Some(dir) = crate::folder_structure::resolve_subdir(root, structure.characters()) {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("md")).unwrap_or(false) {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        push(&mut names, stem);
                    }
                    if let Ok(text) = std::fs::read_to_string(&p) {
                        for line in text.lines().take(20) {
                            let t = line.trim();
                            if let Some(rest) = t.strip_prefix("# ") {
                                push(&mut names, rest);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // Headings under ## Characters in the combined bible blob
    let mut in_chars = false;
    for line in bible.lines() {
        let t = line.trim();
        if t.eq_ignore_ascii_case("## Characters") {
            in_chars = true;
            continue;
        }
        if in_chars && t.starts_with("## ") {
            in_chars = false;
        }
        if in_chars {
            if let Some(rest) = t.strip_prefix("### ") {
                push(&mut names, rest);
            } else if let Some(rest) = t.strip_prefix("# ") {
                push(&mut names, rest);
            }
        }
    }

    names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    names
}

fn clean_name(raw: &str) -> String {
    let s = raw
        .trim()
        .trim_matches(|c: char| c == '*' || c == '_' || c == '#' || c == '"' || c == '\'')
        .split(|c: char| c == '(' || c == '—' || c == '-' || c == ':')
        .next()
        .unwrap_or("")
        .trim();
    // Keep first 1–3 capitalized words (drop file junk)
    let parts: Vec<&str> = s.split_whitespace().take(3).collect();
    parts.join(" ")
}

// ── Dialogue measurement ─────────────────────────────────────────────────────

fn measure_dialogue(content: &str) -> (usize, usize, Vec<(usize, String)>) {
    // Normalize curly quotes to straight for matching
    let text = content
        .replace(['\u{201c}', '\u{201d}'], "\"")
        .replace(['\u{2018}', '\u{2019}'], "'");

    let mut dialogue_words = 0usize;
    let mut quotes: Vec<(usize, String)> = Vec::new();
    let mut chars = text.char_indices();
    let mut in_quote = false;
    let mut quote_start = 0usize;
    let mut quote_buf = String::new();

    while let Some((i, ch)) = chars.next() {
        if ch == '"' {
            if !in_quote {
                in_quote = true;
                quote_start = i;
                quote_buf.clear();
            } else {
                in_quote = false;
                let q = quote_buf.trim().to_string();
                if !q.is_empty() {
                    dialogue_words += q.split_whitespace().count();
                    quotes.push((quote_start, q));
                }
            }
            continue;
        }
        if in_quote {
            quote_buf.push(ch);
        }
    }

    let total_words = text.split_whitespace().count();
    let narrative_words = total_words.saturating_sub(dialogue_words);
    (dialogue_words, narrative_words, quotes)
}

fn attribute_quotes(
    characters: &[String],
    quotes: &[(usize, String)],
    content: &str,
    out: &mut HashMap<String, Vec<String>>,
) {
    let text = content
        .replace(['\u{201c}', '\u{201d}'], "\"")
        .replace(['\u{2018}', '\u{2019}'], "'");
    let lower = text.to_lowercase();

    for (start, quote) in quotes {
        if quote.split_whitespace().count() < 3 {
            continue;
        }
        // Window around the quote for speaker tags: 120 chars before, 80 after
        let win_lo = start.saturating_sub(120);
        let win_hi = (*start + quote.len() + 80).min(text.len());
        let win_lower = lower.get(win_lo..win_hi).unwrap_or("");

        let mut best: Option<&String> = None;
        for name in characters {
            let n = name.to_lowercase();
            // Prefer first name token for matching "said Alice"
            let first = n.split_whitespace().next().unwrap_or(&n);
            if first.len() < 2 {
                continue;
            }
            if win_lower.contains(first) {
                best = Some(name);
                break;
            }
        }
        if let Some(name) = best {
            if let Some(list) = out.get_mut(name) {
                if list.len() >= MAX_SAMPLES_PER_CHAR {
                    continue;
                }
                let clipped: String = quote.chars().take(MAX_SAMPLE_CHARS).collect();
                if !list.iter().any(|e| e == &clipped) {
                    list.push(clipped);
                }
            }
        }
    }
}

fn parse_voice_flags(raw: &str) -> Result<Vec<serde_json::Value>, String> {
    let clean = extract_json_object(raw).ok_or_else(|| "No JSON in voice check response.".to_string())?;
    let parsed: serde_json::Value =
        serde_json::from_str(&clean).map_err(|e| format!("JSON parse: {}", e))?;

    let arr = parsed
        .get("flags")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| parsed.as_array().cloned())
        .unwrap_or_default();

    let mut out = Vec::new();
    for item in arr {
        let characters = item
            .get("characters")
            .cloned()
            .unwrap_or(serde_json::json!([]));
        let summary = item
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let examples = item
            .get("examples")
            .cloned()
            .unwrap_or(serde_json::json!([]));
        let severity = item
            .get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("moderate")
            .to_string();
        if summary.is_empty() && examples.as_array().map(|a| a.is_empty()).unwrap_or(true) {
            continue;
        }
        out.push(serde_json::json!({
            "characters": characters,
            "summary": summary,
            "examples": examples,
            "severity": severity,
        }));
    }
    Ok(out)
}
