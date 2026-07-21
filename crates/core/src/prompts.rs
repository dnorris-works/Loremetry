// prompts.rs — Prompt system: load templates from DB, preprocess text, fill placeholders, call LLM.

use sqlx::PgPool;
use std::collections::HashMap;
use std::path::Path;

use crate::commands::{call_llm, call_llm_json};
use crate::db::Db;
use crate::documents;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PromptTemplate {
    pub id:            String,
    pub system_prompt: String,
    pub user_template: String,
    pub max_tokens:    u32,
    pub json_mode:     bool,
}

pub async fn load_template(pool: &PgPool, template_id: &str) -> Result<PromptTemplate, String> {
    let row: (String, String, String, i64, i64) = sqlx::query_as(
        "SELECT id, system_prompt, user_template, max_tokens, json_mode FROM prompt_templates WHERE id = $1",
    )
    .bind(template_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Prompt template '{}' not found: {}", template_id, e))?
    .ok_or_else(|| format!("Prompt template '{}' not found", template_id))?;

    Ok(PromptTemplate {
        id: row.0,
        system_prompt: row.1,
        user_template: row.2,
        max_tokens: row.3 as u32,
        json_mode: row.4 != 0,
    })
}

pub async fn discover_bible(db: &Db, story_id: &str) -> String {
    truncate_bible(&documents::load_bible_text(&db.0, story_id).await)
}

pub async fn load_bible_for_story(db: &Db, story_id: &str, explicit_bible_path: &str) -> String {
    let discovered = discover_bible(db, story_id).await;
    if !discovered.is_empty() {
        return discovered;
    }
    load_bible(explicit_bible_path)
}

pub fn load_bible(bible_path: &str) -> String {
    if bible_path.is_empty() {
        return String::new();
    }
    let path = Path::new(bible_path);
    if !path.exists() {
        return String::new();
    }
    match std::fs::read_to_string(path) {
        Ok(text) => truncate_bible(&text),
        Err(_) => String::new(),
    }
}

fn truncate_bible(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() > 8000 {
        words[..8000].join(" ") + "\n[Bible truncated]"
    } else {
        text.to_string()
    }
}

pub async fn get_preprocessed(
    pool: &PgPool,
    story_id: &str,
    chapter_file: &str,
    report_type: &str,
    source_mtime: &str,
) -> Option<String> {
    let cached: Option<(String, String)> = sqlx::query_as(
        "SELECT processed_text, source_modified_at FROM preprocessed_chapters
         WHERE story_id = $1 AND chapter_file = $2 AND report_type = $3",
    )
    .bind(story_id)
    .bind(chapter_file)
    .bind(report_type)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    if let Some((text, cached_mtime)) = cached {
        if cached_mtime == source_mtime {
            return Some(text);
        }
    }
    None
}

pub async fn store_preprocessed(
    pool: &PgPool,
    story_id: &str,
    chapter_file: &str,
    report_type: &str,
    processed_text: &str,
    source_mtime: &str,
) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = sqlx::query(
        "INSERT INTO preprocessed_chapters (story_id, chapter_file, report_type, processed_text, source_modified_at, created_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (story_id, chapter_file, report_type)
         DO UPDATE SET processed_text = EXCLUDED.processed_text, source_modified_at = EXCLUDED.source_modified_at, created_at = EXCLUDED.created_at",
    )
    .bind(story_id)
    .bind(chapter_file)
    .bind(report_type)
    .bind(processed_text)
    .bind(source_mtime)
    .bind(&now)
    .execute(pool)
    .await;
}

pub fn fill_template(template: &str, vars: &HashMap<&str, &str>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{}}}", key), value);
    }
    let re_unfilled = regex::Regex::new(r"\{[a-z_]+\}").unwrap();
    re_unfilled.replace_all(&result, "").to_string()
}

pub async fn execute_prompt(
    db: &Db,
    template_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    vars: HashMap<&str, &str>,
) -> Result<String, String> {
    let template = load_template(&db.0, template_id).await?;

    let system_prompt = fill_template(&template.system_prompt, &vars);
    let user_content = fill_template(&template.user_template, &vars);

    if template.json_mode {
        call_llm_json(provider, api_key, model, &system_prompt, &user_content, template.max_tokens).await
    } else {
        call_llm(provider, api_key, model, &system_prompt, &user_content, template.max_tokens).await
    }
}

#[allow(dead_code)]
pub fn preprocess_for_continuity(content: &str) -> String {
    truncate_words(content, 4000)
}

pub fn preprocess_for_sdt(content: &str) -> String {
    truncate_words(content, 4000)
}

pub fn preprocess_for_ai_isms(content: &str) -> String {
    truncate_words(content, 4000)
}

#[allow(dead_code)]
pub fn preprocess_for_genre(content: &str) -> String {
    truncate_words(content, 2000)
}

fn truncate_words(text: &str, max: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max {
        return text.to_string();
    }
    words[..max].join(" ")
}
