// batch_prompt.rs — Batched per-chapter LLM calls with word-budget chunking and single-chapter fallback.

use std::collections::HashMap;

use crate::analysis::extract_json_object;
use crate::app_ctx::AppCtx;
use crate::commands::{call_llm, call_llm_json};
use crate::db::Db;
use crate::prompts;

/// Default total words of chapter text per batch (excluding bible + overhead).
pub const DEFAULT_WORD_BUDGET: usize = 12_000;
/// Tighter budget for craft/publish per-chapter checks (longer excerpts per chapter).
pub const CRAFT_BATCH_WORD_BUDGET: usize = 8_000;

#[derive(Clone, Debug)]
pub struct BatchChapterItem {
    pub file:  String,
    pub title: String,
    pub text:  String,
}

#[derive(Clone, Debug)]
pub struct CachedBatchItem {
    pub item:        BatchChapterItem,
    pub source_hash: String,
}

impl BatchChapterItem {
    pub fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }
}

/// Split items into batches where the sum of chapter word counts stays within `budget`.
pub fn chunk_by_word_budget(items: &[BatchChapterItem], budget: usize) -> Vec<Vec<usize>> {
    let budget = budget.max(1);
    let mut chunks: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut current_words = 0usize;

    for (i, item) in items.iter().enumerate() {
        let words = item.word_count().max(1);
        if !current.is_empty() && current_words + words > budget {
            chunks.push(current);
            current = Vec::new();
            current_words = 0;
        }
        current.push(i);
        current_words += words;
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

/// Human-readable block listing chapters for the batch user message.
pub fn format_chapters_block(items: &[BatchChapterItem]) -> String {
    let mut out = String::new();
    for item in items {
        out.push_str(&format!(
            "=== FILE: {} ===\nTitle: {}\nWords: {}\n\n{}\n\n",
            item.file,
            item.title,
            item.word_count(),
            item.text
        ));
    }
    out.trim_end().to_string()
}

fn scaled_max_tokens(base: u32, chapter_count: usize) -> u32 {
    let scaled = base.saturating_mul(chapter_count as u32);
    scaled.clamp(base, 16_000)
}

/// Execute a batch prompt template with `{bible}`, `{chapters_block}`, and `{chapter_count}` filled.
pub async fn execute_batch_prompt(
    app: &AppCtx,
    template_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible: &str,
    items: &[BatchChapterItem],
    story_id: &str,
) -> Result<String, String> {
    let chapters_block = format_chapters_block(items);
    let chapter_count = items.len().to_string();
    let mut vars = HashMap::new();
    vars.insert("bible", bible);
    vars.insert("chapters_block", chapters_block.as_str());
    vars.insert("chapter_count", chapter_count.as_str());

    let template = prompts::load_template(&app.db.pool, template_id).await?;
    let max_tokens = scaled_max_tokens(template.max_tokens, items.len());
    let system_prompt = prompts::fill_template(&template.system_prompt, &vars);
    let user_content = prompts::fill_template(&template.user_template, &vars);

    let result = if template.json_mode {
        call_llm_json(provider, api_key, model, &system_prompt, &user_content, max_tokens).await?
    } else {
        call_llm(provider, api_key, model, &system_prompt, &user_content, max_tokens).await?
    };

    let _ = app
        .usage
        .record_llm(
            app.user_id(),
            provider,
            model,
            template_id,
            Some(story_id),
            result.usage,
        )
        .await;

    Ok(result.text)
}

async fn execute_single_chapter_prompt(
    app: &AppCtx,
    template_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible: &str,
    item: &BatchChapterItem,
    extra_vars: &[(&str, &str)],
    story_id: &str,
) -> Result<String, String> {
    let mut vars = HashMap::new();
    let computed = format!("Word count: {}", item.word_count());
    vars.insert("chapter_title", item.title.as_str());
    vars.insert("chapter_text", item.text.as_str());
    vars.insert("bible", bible);
    vars.insert("computed_signals", computed.as_str());
    for (key, value) in extra_vars {
        vars.insert(*key, *value);
    }
    prompts::execute_prompt(app, template_id, provider, api_key, model, vars, Some(story_id)).await
}

/// Normalize a single-chapter LLM response into the standard per-chapter JSON shape.
pub fn wrap_single_response(template_id: &str, raw: &str) -> Result<serde_json::Value, String> {
    let clean = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    match template_id {
        "chapter_summary" => {
            if clean.is_empty() {
                return Err("Empty summary returned.".into());
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(clean) {
                if let Some(s) = crate::analysis::chapters::render_genre_signals(&v) {
                    return Ok(serde_json::json!({ "summary": s }));
                }
            }
            Ok(serde_json::json!({ "summary": clean }))
        }
        "sdt_check" | "ai_isms_check" => {
            let findings = parse_json_array(clean)?;
            Ok(serde_json::json!({ "findings": findings }))
        }
        "craft_prose_checks" | "craft_prose_checks_single" => {
            let sdt = parse_json_array_field(clean, "sdt_findings");
            let ai = parse_json_array_field(clean, "ai_isms_findings");
            Ok(serde_json::json!({ "sdt_findings": sdt, "ai_isms_findings": ai }))
        }
        "continuity_extract" => {
            let facts = parse_json_array(clean)?;
            Ok(serde_json::json!({ "facts": facts }))
        }
        _ => {
            let obj = extract_json_object(clean).unwrap_or_else(|| clean.to_string());
            serde_json::from_str(&obj)
                .map_err(|e| format!("Parse error: {} | {}", e, &obj[..obj.len().min(200)]))
        }
    }
}

fn parse_json_array_field(clean: &str, field: &str) -> Vec<serde_json::Value> {
    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(clean) {
        if let Some(arr) = obj.get(field).and_then(|v| v.as_array()) {
            return arr.clone();
        }
    }
    if field == "sdt_findings" || field == "findings" {
        return parse_json_array(clean).unwrap_or_default();
    }
    Vec::new()
}

fn parse_json_array(clean: &str) -> Result<Vec<serde_json::Value>, String> {
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(clean) {
        return Ok(arr);
    }
    let json_str = if clean.starts_with('[') {
        clean.to_string()
    } else if let Some(start) = clean.find('[') {
        extract_bracketed_array(clean, start)
    } else {
        clean.to_string()
    };
    serde_json::from_str(&json_str).map_err(|e| {
        format!("Array parse error: {} | {}", e, &json_str[..json_str.len().min(200)])
    })
}

fn extract_bracketed_array(clean: &str, start: usize) -> String {
    let bytes = clean.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut end = clean.len();
    for (i, &b) in bytes[start..].iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        match b {
            b'\\' if in_string => escape = true,
            b'"' => in_string = !in_string,
            b'[' if !in_string => depth += 1,
            b']' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    end = start + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    clean[start..end].to_string()
}

/// Extract a string field from a per-chapter batch value object.
pub fn chapter_string_field(value: &serde_json::Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Extract a JSON array field from a per-chapter batch value object.
pub fn chapter_array_field(value: &serde_json::Value, field: &str) -> Vec<serde_json::Value> {
    value
        .get(field)
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

/// Run batched calls with per-chapter fallback when a batch fails or omits files.
pub async fn process_chapters_batched(
    app: &AppCtx,
    database: &Db,
    provider: &str,
    api_key: &str,
    model: &str,
    batch_template_id: &str,
    single_template_id: &str,
    bible: &str,
    story_id: &str,
    cache_report_type: Option<&str>,
    items: Vec<CachedBatchItem>,
    word_budget: usize,
    single_extra: &[(&str, &str)],
) -> HashMap<String, serde_json::Value> {
    if items.is_empty() {
        return HashMap::new();
    }

    let mut results: HashMap<String, serde_json::Value> = HashMap::new();
    let mut to_process: Vec<CachedBatchItem> = Vec::new();

    if let Some(report_type) = cache_report_type {
        for entry in items {
            if let Some(cached) = crate::db::load_chapter_ai_cache(
                &database.pool,
                story_id,
                &entry.item.file,
                report_type,
                &entry.source_hash,
            )
            .await
            {
                if let Ok(value) = serde_json::from_str(&cached) {
                    results.insert(entry.item.file.clone(), value);
                    continue;
                }
            }
            to_process.push(entry);
        }
    } else {
        to_process = items;
    }

    if to_process.is_empty() {
        return results;
    }

    let fresh = if to_process.len() == 1 {
        let entry = &to_process[0];
        match run_single(
            app,
            provider,
            api_key,
            model,
            single_template_id,
            bible,
            &entry.item,
            single_extra,
            story_id,
        )
        .await
        {
            Ok(v) => HashMap::from([(entry.item.file.clone(), v)]),
            Err(_) => HashMap::new(),
        }
    } else {
        run_batches(
            app,
            database,
            provider,
            api_key,
            model,
            batch_template_id,
            single_template_id,
            bible,
            story_id,
            &to_process,
            word_budget,
            single_extra,
        )
        .await
    };

    if let Some(report_type) = cache_report_type {
        for (file, value) in &fresh {
            if let Some(hash) = to_process
                .iter()
                .find(|e| e.item.file == *file)
                .map(|e| e.source_hash.as_str())
            {
                if let Ok(json) = serde_json::to_string(value) {
                    crate::db::save_chapter_ai_cache(
                        &database.pool,
                        story_id,
                        file,
                        report_type,
                        hash,
                        &json,
                    )
                    .await;
                }
            }
        }
    }

    results.extend(fresh);
    results
}

async fn run_batches(
    app: &AppCtx,
    database: &Db,
    provider: &str,
    api_key: &str,
    model: &str,
    batch_template_id: &str,
    single_template_id: &str,
    bible: &str,
    story_id: &str,
    to_process: &[CachedBatchItem],
    word_budget: usize,
    single_extra: &[(&str, &str)],
) -> HashMap<String, serde_json::Value> {
    let batch_items: Vec<BatchChapterItem> = to_process.iter().map(|e| e.item.clone()).collect();
    let chunks = chunk_by_word_budget(&batch_items, word_budget);
    let mut results: HashMap<String, serde_json::Value> = HashMap::new();

    for chunk_indices in chunks {
        if crate::is_cancelled() {
            break;
        }

        let chunk: Vec<CachedBatchItem> = chunk_indices.iter().map(|&i| to_process[i].clone()).collect();
        let chunk_items: Vec<BatchChapterItem> = chunk.iter().map(|e| e.item.clone()).collect();

        let mut parsed = try_batch_parse(
            app,
            batch_template_id,
            provider,
            api_key,
            model,
            bible,
            &chunk_items,
            story_id,
        )
        .await;

        if parsed.is_err() {
            parsed = try_batch_parse(
                app,
                batch_template_id,
                provider,
                api_key,
                model,
                bible,
                &chunk_items,
                story_id,
            )
            .await;
        }

        match parsed {
            Ok(map) if chunk.iter().all(|it| map.contains_key(&it.item.file)) => {
                results.extend(map);
            }
            Ok(map) => {
                for entry in &chunk {
                    if let Some(value) = map.get(&entry.item.file) {
                        results.insert(entry.item.file.clone(), value.clone());
                    } else if let Ok(value) = run_single(
                        app,
                        provider,
                        api_key,
                        model,
                        single_template_id,
                        bible,
                        &entry.item,
                        single_extra,
                        story_id,
                    )
                    .await
                    {
                        results.insert(entry.item.file.clone(), value);
                    }
                }
            }
            Err(_) => {
                for entry in &chunk {
                    if let Ok(value) = run_single(
                        app,
                        provider,
                        api_key,
                        model,
                        single_template_id,
                        bible,
                        &entry.item,
                        single_extra,
                        story_id,
                    )
                    .await
                    {
                        results.insert(entry.item.file.clone(), value);
                    }
                }
            }
        }
    }

    results
}

async fn try_batch_parse(
    app: &AppCtx,
    batch_template_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible: &str,
    chunk_items: &[BatchChapterItem],
    story_id: &str,
) -> Result<HashMap<String, serde_json::Value>, String> {
    let raw = execute_batch_prompt(
        app,
        batch_template_id,
        provider,
        api_key,
        model,
        bible,
        chunk_items,
        story_id,
    )
    .await?;
    parse_batch_chapters_map(&raw)
}

async fn run_single(
    app: &AppCtx,
    provider: &str,
    api_key: &str,
    model: &str,
    single_template_id: &str,
    bible: &str,
    item: &BatchChapterItem,
    single_extra: &[(&str, &str)],
    story_id: &str,
) -> Result<serde_json::Value, String> {
    let raw = execute_single_chapter_prompt(
        app,
        single_template_id,
        provider,
        api_key,
        model,
        bible,
        item,
        single_extra,
        story_id,
    )
    .await?;
    wrap_single_response(single_template_id, &raw)
}

/// Parse `{"chapters": {"file.md": {...}}}` from model output.
pub fn parse_batch_chapters_map(raw: &str) -> Result<HashMap<String, serde_json::Value>, String> {
    let clean = extract_json_object(raw).unwrap_or_else(|| raw.trim().to_string());
    let root: serde_json::Value = serde_json::from_str(&clean)
        .map_err(|e| format!("Batch JSON parse error: {} | {}", e, &clean[..clean.len().min(200)]))?;

    let chapters = root
        .get("chapters")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "Batch response missing \"chapters\" object.".to_string())?;

    Ok(chapters.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_by_word_budget_splits_heavy_chapters() {
        let items = vec![
            BatchChapterItem { file: "a.md".into(), title: "A".into(), text: "word ".repeat(5000) },
            BatchChapterItem { file: "b.md".into(), title: "B".into(), text: "word ".repeat(5000) },
            BatchChapterItem { file: "c.md".into(), title: "C".into(), text: "word ".repeat(1000) },
        ];
        let chunks = chunk_by_word_budget(&items, 6000);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], vec![0]);
        assert_eq!(chunks[1], vec![1, 2]);
    }

    #[test]
    fn parse_batch_chapters_map_reads_file_keys() {
        let raw = r#"{"chapters":{"01.md":{"summary":"Romance"},"02.md":{"summary":"Suspense"}}}"#;
        let map = parse_batch_chapters_map(raw).unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(
            chapter_string_field(&map["01.md"], "summary").as_deref(),
            Some("Romance")
        );
    }

    #[test]
    fn wrap_single_response_renders_chapter_summary_signals() {
        let raw = r#"{"setting":"small town","tone":"warm","tropes":["forced proximity"]}"#;
        let wrapped = wrap_single_response("chapter_summary", raw).unwrap();
        let summary = chapter_string_field(&wrapped, "summary").unwrap();
        assert!(summary.contains("Setting: small town"));
        assert!(summary.contains("Tropes: forced proximity"));
    }

    #[test]
    fn wrap_single_response_extracts_findings_array() {
        let raw = "Here you go:\n[{\"quote\":\"a\"},{\"quote\":\"b\"}]\nDone.";
        let wrapped = wrap_single_response("sdt_check", raw).unwrap();
        assert_eq!(chapter_array_field(&wrapped, "findings").len(), 2);
    }

    #[test]
    fn wrap_single_response_splits_craft_fields() {
        let raw = r#"{"sdt_findings":[{"quote":"a"}],"ai_isms_findings":[]}"#;
        let wrapped = wrap_single_response("craft_prose_checks_single", raw).unwrap();
        assert_eq!(chapter_array_field(&wrapped, "sdt_findings").len(), 1);
        assert!(chapter_array_field(&wrapped, "ai_isms_findings").is_empty());
    }
}
