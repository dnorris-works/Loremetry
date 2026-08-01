// commands.rs — Command handlers and async LLM client

use std::collections::HashMap;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::app_ctx::AppCtx;
use crate::documents;
use crate::usage::LlmUsage;

#[derive(Debug, Clone, Default)]
pub struct LlmResult {
    pub text: String,
    pub usage: LlmUsage,
}

impl LlmResult {
    fn from_text(text: String) -> Self {
        Self {
            text,
            usage: LlmUsage::default(),
        }
    }
}

// ── Shared result types ───────────────────────────────────────────────────────

#[derive(Serialize, Clone, Default)]
pub struct CategoryStatRow {
    pub requested_path: String,
    pub matched_path:   String,
    pub found:          bool,
    pub sales_to_one:   String,
    pub sales_to_ten:   String,
    pub publisher_pct:  String,
    pub ku_pct:         String,
    pub top_books:      Vec<crate::canopy::TopBook>,
}

#[derive(Serialize)]
pub struct AnalyzerResult {
    pub success:  bool,
    pub markdown: String,
    pub error:    String,
    #[serde(default)]
    pub rows:     Vec<CategoryStatRow>,
}

// ── CSV Analyzer ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CsvRequest {
    pub keyword: String,
    pub csv_content: String,
    pub api_key: String,
    pub model: String,
    pub provider: String,
}

pub async fn analyze_csv(
    app: AppCtx,
    request: CsvRequest,
) -> AnalyzerResult {
    let _ = app.emit("cdp:log", &format!("Running CSV Analyzer for keyword: {} [{}]...", request.keyword, request.model));

    let database = app.db.as_ref();
    let mut vars = HashMap::new();
    vars.insert("keyword", request.keyword.as_str());
    vars.insert("csv_content", request.csv_content.as_str());

    match crate::prompts::execute_prompt(
        &app,
        "csv_competition_analysis",
        &request.provider,
        &request.api_key,
        &request.model,
        vars,
        None,
    )
    .await {
        Ok(markdown) => {
            let _ = app.emit("cdp:log", "✓ CSV analysis complete.");
            AnalyzerResult { success: true, markdown, error: String::new(), rows: Vec::new() }
        }
        Err(e) => AnalyzerResult { success: false, markdown: String::new(), error: e, rows: Vec::new() },
    }
}

// ── AI client (async) ─────────────────────────────────────────────────────────

/// Call the LLM asynchronously. Cancellable via tokio::select! — dropping the
/// future closes the HTTP connection immediately.
pub async fn call_llm(
    provider: &str,
    api_key: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<LlmResult, String> {
    match provider {
        "tokenmix" => call_tokenmix(api_key, model, system, user, max_tokens, false).await,
        _ => call_claude(api_key, model, system, user, max_tokens).await,
    }
}

/// Same as call_llm but forces JSON mode (valid JSON guaranteed in response).
pub async fn call_llm_json(
    provider: &str,
    api_key: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<LlmResult, String> {
    match provider {
        "tokenmix" => call_tokenmix(api_key, model, system, user, max_tokens, true).await,
        _ => call_claude(api_key, model, system, user, max_tokens).await,
    }
}

async fn call_claude(
    api_key: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<LlmResult, String> {
    let body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": system,
        "messages": [{"role": "user", "content": user}]
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Claude request failed: {}", e))?;

    let json: Value = resp.json()
        .await
        .map_err(|e| format!("Claude response parse failed: {}", e))?;

    if let Some(err) = json.get("error") {
        return Err(format!("Claude API error: {}", err["message"].as_str().unwrap_or("unknown")));
    }

    let text = json["content"][0]["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Claude: empty response".to_string())?;

    let usage = json.get("usage");
    let input_tokens = usage
        .and_then(|u| u["input_tokens"].as_u64())
        .unwrap_or(0) as u32;
    let output_tokens = usage
        .and_then(|u| u["output_tokens"].as_u64())
        .unwrap_or(0) as u32;

    Ok(LlmResult {
        text,
        usage: LlmUsage {
            input_tokens,
            output_tokens,
        },
    })
}

async fn call_tokenmix(
    api_key: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: u32,
    json_mode: bool,
) -> Result<LlmResult, String> {
    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]
    });

    if json_mode {
        body["response_format"] = json!({"type": "json_object"});
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post("https://api.tokenmix.ai/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("TokenMix request failed: {}", e))?;

    let json: Value = resp.json()
        .await
        .map_err(|e| format!("TokenMix response parse failed: {}", e))?;

    if let Some(err) = json.get("error") {
        let msg = err["message"].as_str().unwrap_or(
            err.as_str().unwrap_or("unknown error")
        );
        return Err(format!("TokenMix error: {}", msg));
    }

    let text = json["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "TokenMix: empty response".to_string())?;

    let usage = json.get("usage");
    let input_tokens = usage
        .and_then(|u| u["prompt_tokens"].as_u64())
        .unwrap_or(0) as u32;
    let output_tokens = usage
        .and_then(|u| u["completion_tokens"].as_u64())
        .unwrap_or(0) as u32;

    Ok(LlmResult {
        text,
        usage: LlmUsage {
            input_tokens,
            output_tokens,
        },
    })
}

// ── List models (async) ───────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ModelsResult {
    pub success: bool,
    pub models: Vec<ModelInfo>,
    pub error: String,
}

#[derive(Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub owned_by: String,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
}

pub async fn list_models(
    db: &crate::db::Db,
    provider: String,
    api_key: String,
) -> Result<ModelsResult, String> {
    Ok(match provider.as_str() {
        "tokenmix" => fetch_tokenmix_models(&api_key).await,
        "claude" => fetch_claude_models(db).await,
        _ => ModelsResult {
            success: false, models: Vec::new(),
            error: format!("Unknown provider: {}", provider),
        },
    })
}

async fn fetch_tokenmix_models(api_key: &str) -> ModelsResult {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => return ModelsResult { success: false, models: Vec::new(), error: format!("Client error: {}", e) },
    };

    // Use the new API endpoint with type=llm filter to get only chat models with pricing
    let resp = match client
        .get("https://aihubmix.com/api/v1/models?type=llm")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => {
            // Fallback to legacy endpoint if new API fails
            return fetch_tokenmix_models_legacy(&client, api_key).await;
        }
    };

    let json: Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return fetch_tokenmix_models_legacy(&client, api_key).await,
    };

    if let Some(err) = json.get("error") {
        let msg = err["message"].as_str().unwrap_or("unknown");
        return ModelsResult {
            success: false, models: Vec::new(),
            error: format!("API error: {}", msg),
        };
    }

    let models: Vec<ModelInfo> = json["data"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|m| {
            let id = m["model_id"].as_str()
                .or_else(|| m["id"].as_str())
                .unwrap_or("");
            if id.is_empty() { return None; }

            // Pricing: pass through raw values from API
            let input_price = m["pricing"]["input"].as_f64();
            let output_price = m["pricing"]["output"].as_f64();

            Some(ModelInfo {
                id: id.to_string(),
                owned_by: m["owned_by"].as_str()
                    .or_else(|| m["desc"].as_str().map(|d| &d[..d.len().min(40)]))
                    .unwrap_or("")
                    .to_string(),
                input_price,
                output_price,
            })
        })
        .collect();

    if models.is_empty() {
        return fetch_tokenmix_models_legacy(&client, api_key).await;
    }

    ModelsResult { success: true, models, error: String::new() }
}

/// Legacy /v1/models endpoint fallback (no pricing, no type filter)
async fn fetch_tokenmix_models_legacy(client: &reqwest::Client, api_key: &str) -> ModelsResult {
    let resp = match client
        .get("https://api.tokenmix.ai/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return ModelsResult {
            success: false, models: Vec::new(),
            error: format!("Request failed: {}", e),
        },
    };

    let json: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return ModelsResult {
            success: false, models: Vec::new(),
            error: format!("Parse failed: {}", e),
        },
    };

    if let Some(err) = json.get("error") {
        let msg = err["message"].as_str().unwrap_or("unknown");
        return ModelsResult {
            success: false, models: Vec::new(),
            error: format!("API error: {}", msg),
        };
    }

    let models: Vec<ModelInfo> = json["data"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .map(|m| {
            let input_price = m["pricing"]["input"].as_f64()
                .or_else(|| m["pricing"]["prompt"].as_f64());
            let output_price = m["pricing"]["output"].as_f64()
                .or_else(|| m["pricing"]["completion"].as_f64());
            ModelInfo {
                id: m["id"].as_str().unwrap_or("").to_string(),
                owned_by: m["owned_by"].as_str().unwrap_or("").to_string(),
                input_price,
                output_price,
            }
        })
        .filter(|m| !m.id.is_empty())
        .collect();

    ModelsResult { success: true, models, error: String::new() }
}

async fn fetch_claude_models(db: &crate::db::Db) -> ModelsResult {
    let models = crate::db::list_provider_models(&db.pool, "claude")
        .await
        .into_iter()
        .map(|m| ModelInfo {
            id: m.id,
            owned_by: m.owned_by,
            input_price: m.input_price,
            output_price: m.output_price,
        })
        .collect::<Vec<_>>();
    if models.is_empty() {
        return ModelsResult {
            success: false,
            models: Vec::new(),
            error: "No Claude models seeded in provider_models.".to_string(),
        };
    }
    ModelsResult { success: true, models, error: String::new() }
}

// ── Manuscript document operations ────────────────────────────────────────────

/// Read a chapter document by id. Returns the full text content.
pub async fn read_chapter(app: AppCtx, doc_id: i64) -> Result<String, String> {
    let doc = documents::get_document(&app.db.pool, doc_id)
        .await?
        .ok_or_else(|| format!("Document {} not found", doc_id))?;
    Ok(doc.content)
}

/// Save a chapter document (full overwrite). Used by the editor's auto-save.
pub async fn save_chapter(app: AppCtx, doc_id: i64, content: String) -> Result<(), String> {
    let doc = documents::get_document(&app.db.pool, doc_id)
        .await?
        .ok_or_else(|| format!("Document {} not found", doc_id))?;
    let req = documents::UpsertDocumentRequest {
        story_id: doc.story_id,
        kind: doc.kind,
        title: doc.title,
        path_hint: doc.path_hint,
        content,
        id: Some(doc_id),
    };
    documents::upsert_document(&app.db.pool, &req).await?;
    Ok(())
}

/// Apply a text fix to a manuscript document.
/// Finds `old_text` in the document and replaces it with `new_text`.
/// Returns the updated full content so the UI can refresh.
pub async fn write_manuscript_fix(
    app: AppCtx,
    doc_id: i64,
    old_text: String,
    new_text: String,
) -> Result<String, String> {
    let doc = documents::write_document_fix(&app.db.pool, doc_id, &old_text, &new_text).await?;
    Ok(doc.content)
}

// ── Manuscript file tree ──────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
pub struct FileTreeEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<FileTreeEntry>,
}

/// Returns manuscript documents for a story as a flat file-tree list.
/// `path` is `doc:{id}`; name prefers path_hint then title.
pub async fn list_manuscript_files(app: AppCtx, story_id: String) -> Result<Vec<FileTreeEntry>, String> {
    if !crate::stories::story_exists(&app.db, &story_id).await {
        return Err(format!("Story not found: {}", story_id));
    }
    let docs = documents::list_documents_db(&app.db, &story_id).await?;
    let mut entries: Vec<FileTreeEntry> = docs
        .into_iter()
        .filter(|d| d.kind == "chapter" || d.kind == "bible" || d.kind == "character" || d.kind == "location")
        .map(|d| {
            let name = if !d.path_hint.is_empty() {
                d.path_hint.clone()
            } else if !d.title.is_empty() {
                d.title.clone()
            } else {
                format!("{}-{}", d.kind, d.id)
            };
            FileTreeEntry {
                name,
                path: format!("doc:{}", d.id),
                is_dir: false,
                children: Vec::new(),
            }
        })
        .collect();
    entries.sort_by(|a, b| natural_sort_cmp(&a.name, &b.name));
    Ok(entries)
}

fn natural_sort_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    fn key(s: &str) -> Vec<u64> {
        let mut k = Vec::new();
        let mut num = String::new();
        for c in s.chars() {
            if c.is_ascii_digit() {
                num.push(c);
            } else {
                if !num.is_empty() {
                    k.push(num.parse::<u64>().unwrap_or(0));
                    num.clear();
                }
                k.push(c.to_lowercase().next().unwrap_or(c) as u64 + 1_000_000);
            }
        }
        if !num.is_empty() { k.push(num.parse::<u64>().unwrap_or(0)); }
        k
    }
    key(a).cmp(&key(b))
}

// ── Cost estimation ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CostEstimateRequest {
    #[serde(alias = "folder")]
    pub story_id: String,
    /// Map of report_id → (input_price_per_token, output_price_per_token)
    /// Prices as returned by the API (per-1K tokens).
    pub model_prices: Vec<ReportModelPrice>,
}

#[derive(Deserialize)]
pub struct ReportModelPrice {
    pub report_id:    String,
    pub input_price:  f64,  // per 1K tokens
    pub output_price: f64,  // per 1K tokens
}

#[derive(Serialize)]
pub struct CostEstimateResult {
    pub success: bool,
    pub chapter_count: usize,
    pub total_words: usize,
    pub estimates: Vec<ReportCostEstimate>,
    pub error: String,
}

#[derive(Serialize)]
pub struct ReportCostEstimate {
    pub report_id: String,
    pub estimated_cost: f64,  // in USD
    pub calls: usize,
    pub input_tokens: usize,
    pub output_tokens: usize,
}

/// Estimate the AI cost for each report based on manuscript size and model pricing.
pub async fn estimate_report_costs(
    app: AppCtx,
    request: CostEstimateRequest,
) -> Result<CostEstimateResult, String> {
    if !crate::stories::story_exists(&app.db, &request.story_id).await {
        return Ok(CostEstimateResult {
            success: false, chapter_count: 0, total_words: 0,
            estimates: Vec::new(), error: "Story not found.".to_string(),
        });
    }

    let chapters = documents::list_chapters_db(&app.db, &request.story_id).await.unwrap_or_default();
    let chapter_count = chapters.len();

    // Count words per chapter
    let word_counts: Vec<usize> = chapters.iter().map(|c| {
        c.content.split_whitespace().count()
    }).collect();
    let total_words: usize = word_counts.iter().sum();

    // Token estimation constants
    const WORDS_TO_TOKENS: f64 = 1.3;  // average for English prose
    const SYSTEM_PROMPT_TOKENS: usize = 400;  // approximate for our prompts

    let mut cost_params = std::collections::HashMap::new();
    for rp in &request.model_prices {
        let p = crate::db::load_report_cost_params(&app.db.pool, &rp.report_id).await;
        cost_params.insert(rp.report_id.clone(), p);
    }

    let mut estimates = Vec::new();

    for rp in &request.model_prices {
        let params = cost_params.get(&rp.report_id).cloned().unwrap_or_default();

        // Skip non-AI reports
        if params.output_max == 0 && params.fixed_calls == 0 && !params.per_chapter {
            estimates.push(ReportCostEstimate {
                report_id: rp.report_id.clone(),
                estimated_cost: 0.0,
                calls: 0,
                input_tokens: 0,
                output_tokens: 0,
            });
            continue;
        }

        let (calls, total_input_tokens, total_output_tokens) = if params.per_chapter {
            // Per-chapter reports: sum input tokens across all chapters
            let input_tokens: usize = word_counts.iter().map(|&wc| {
                let truncated = if params.truncation > 0 { wc.min(params.truncation) } else { wc };
                (truncated as f64 * WORDS_TO_TOKENS) as usize + SYSTEM_PROMPT_TOKENS
            }).sum();
            let output_tokens = chapter_count * params.output_max;
            let calls = chapter_count + params.fixed_calls;
            (calls, input_tokens, output_tokens)
        } else {
            // Fixed-call reports: use a rough input estimate
            let input_tokens = params.fixed_calls * (2000 + SYSTEM_PROMPT_TOKENS);
            let output_tokens = params.fixed_calls * params.output_max;
            (params.fixed_calls, input_tokens, output_tokens)
        };

        // Cost = (input_tokens / 1000 * input_price) + (output_tokens / 1000 * output_price)
        let cost = (total_input_tokens as f64 / 1000.0 * rp.input_price)
                 + (total_output_tokens as f64 / 1000.0 * rp.output_price);

        estimates.push(ReportCostEstimate {
            report_id: rp.report_id.clone(),
            estimated_cost: (cost * 1000.0).round() / 1000.0,  // round to 3 decimal places
            calls,
            input_tokens: total_input_tokens,
            output_tokens: total_output_tokens,
        });
    }

    Ok(CostEstimateResult {
        success: true,
        chapter_count,
        total_words,
        estimates,
        error: String::new(),
    })
}

#[derive(Deserialize)]
pub struct SummaryRefreshEstimateRequest {
    #[serde(alias = "folder")]
    pub story_id: String,
    #[serde(default)]
    pub input_price: Option<f64>,
    #[serde(default)]
    pub output_price: Option<f64>,
}

#[derive(Serialize)]
pub struct SummaryRefreshEstimateResult {
    pub success: bool,
    pub files: Vec<String>,
    pub chapter_count: usize,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub estimated_cost: Option<f64>,
    pub error: String,
}

/// Estimate the one-time cost to refresh chapter summaries for changed/new chapters only.
pub async fn estimate_summary_refresh_cost(
    app: AppCtx,
    request: SummaryRefreshEstimateRequest,
) -> Result<SummaryRefreshEstimateResult, String> {
    const WORDS_TO_TOKENS: f64 = 1.3;
    const SYSTEM_PROMPT_TOKENS: usize = 400;
    const SUMMARY_OUTPUT_TOKENS: usize = 600;

    if !crate::stories::story_exists(&app.db, &request.story_id).await {
        return Ok(SummaryRefreshEstimateResult {
            success: false,
            files: Vec::new(),
            chapter_count: 0,
            input_tokens: 0,
            output_tokens: 0,
            estimated_cost: None,
            error: "Story not found.".to_string(),
        });
    }

    let chapters = documents::list_chapters_db(&app.db, &request.story_id)
        .await
        .unwrap_or_default();
    let summary_hashes = crate::db::load_chapter_summary_hashes(&app.db.pool, &request.story_id).await;
    let trunc_limit = {
        let params = crate::db::load_report_cost_params(&app.db.pool, "chapter_summaries").await;
        if params.truncation > 0 { params.truncation } else { 2000 }
    };

    let mut files_to_refresh: Vec<String> = Vec::new();
    let mut input_tokens = 0usize;
    let mut output_tokens = 0usize;

    for chapter in &chapters {
        let file = documents::chapter_display_name(chapter);
        let cleaned = crate::manuscript_fingerprint::clean_for_ai(&chapter.content);
        if cleaned.is_empty() {
            continue;
        }
        let hash = crate::manuscript_fingerprint::chapter_source_hash(&cleaned);
        let needs_refresh = match summary_hashes.get(&file) {
            None => true,
            Some(stored) if stored.is_empty() || stored != &hash => true,
            Some(_) => false,
        };
        if needs_refresh {
            files_to_refresh.push(file);
            let word_count = cleaned.split_whitespace().count();
            let truncated = word_count.min(trunc_limit);
            input_tokens += (truncated as f64 * WORDS_TO_TOKENS) as usize + SYSTEM_PROMPT_TOKENS;
            output_tokens += SUMMARY_OUTPUT_TOKENS;
        }
    }

    let estimated_cost = match (request.input_price, request.output_price) {
        (Some(input_price), Some(output_price))
            if input_price >= 0.0 && output_price >= 0.0 =>
        {
            if input_tokens > 0 || output_tokens > 0 {
                let cost = (input_tokens as f64 / 1000.0 * input_price)
                    + (output_tokens as f64 / 1000.0 * output_price);
                Some((cost * 1000.0).round() / 1000.0)
            } else {
                Some(0.0)
            }
        }
        _ => None,
    };

    Ok(SummaryRefreshEstimateResult {
        success: true,
        files: files_to_refresh.clone(),
        chapter_count: files_to_refresh.len(),
        input_tokens,
        output_tokens,
        estimated_cost,
        error: String::new(),
    })
}

// ── AI Chat with context ──────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ChatMessage {
    pub role: String,   // "user" or "assistant"
    pub content: String,
}

#[derive(Deserialize)]
pub struct ChatRequest {
    pub provider:       String,
    #[serde(default)]
    pub api_key:        String,
    pub model:          String,
    pub message:        String,
    pub chapter_text:   String,
    pub chapter_title:  String,
    pub bible:          String,
    pub history:        Vec<ChatMessage>,
}

#[derive(Serialize)]
pub struct ChatResponse {
    pub success: bool,
    pub reply:   String,
    pub error:   String,
}

/// Contextual AI chat for the Writing panel.
/// Sends the user's message with the current chapter and bible as context.
pub async fn chat_with_context(
    app: &AppCtx,
    request: ChatRequest,
) -> Result<ChatResponse, ()> {
    if request.api_key.is_empty() {
        return Ok(ChatResponse {
            success: false,
            reply: String::new(),
            error: "Platform API keys not configured (Admin).".to_string(),
        });
    }
    if request.model.is_empty() {
        return Ok(ChatResponse {
            success: false,
            reply: String::new(),
            error: "No model selected. Go to Settings.".to_string(),
        });
    }

    let template = match crate::prompts::load_template(&app.db.pool, "writing_chat").await {
        Ok(t) => t,
        Err(e) => return Ok(ChatResponse { success: false, reply: String::new(), error: e }),
    };

    let bible_section = if request.bible.is_empty() {
        String::new()
    } else {
        format!("Story Bible:\n{}\n\n---", request.bible)
    };
    let chapter_text = if request.chapter_text.len() > 12000 {
        format!("{}...[truncated]", &request.chapter_text[..12000])
    } else {
        request.chapter_text.clone()
    };

    let mut vars = HashMap::new();
    vars.insert("bible_section", bible_section.as_str());
    vars.insert("chapter_title", request.chapter_title.as_str());
    vars.insert("chapter_text", chapter_text.as_str());

    let system = crate::prompts::fill_template(&template.system_prompt, &vars);

    // Build messages array: system + history + new user message
    let mut messages = Vec::new();
    messages.push(json!({"role": "system", "content": system}));
    for msg in &request.history {
        messages.push(json!({"role": msg.role, "content": msg.content}));
    }
    messages.push(json!({"role": "user", "content": request.message}));

    let body = json!({
        "model": request.model,
        "max_tokens": 2000,
        "messages": messages,
    });

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
    {
        Ok(c) => c,
        Err(e) => return Ok(ChatResponse { success: false, reply: String::new(), error: format!("Client error: {}", e) }),
    };

    let base_url = match request.provider.as_str() {
        "claude" => "https://api.anthropic.com/v1/messages",
        _ => "https://api.tokenmix.ai/v1/chat/completions",
    };

    // For Claude, use their native API format
    if request.provider == "claude" {
        let claude_messages: Vec<serde_json::Value> = request.history.iter()
            .map(|m| json!({"role": m.role, "content": m.content}))
            .chain(std::iter::once(json!({"role": "user", "content": request.message})))
            .collect();

        let claude_body = json!({
            "model": request.model,
            "max_tokens": 2000,
            "system": system,
            "messages": claude_messages,
        });

        let resp = match client.post(base_url)
            .header("x-api-key", &request.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&claude_body)
            .send().await
        {
            Ok(r) => r,
            Err(e) => return Ok(ChatResponse { success: false, reply: String::new(), error: format!("Request failed: {}", e) }),
        };

        let json: Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => return Ok(ChatResponse { success: false, reply: String::new(), error: format!("Parse failed: {}", e) }),
        };

        if let Some(err) = json.get("error") {
            return Ok(ChatResponse { success: false, reply: String::new(), error: format!("Claude: {}", err["message"].as_str().unwrap_or("unknown")) });
        }

        let reply = json["content"][0]["text"].as_str().unwrap_or("").to_string();
        let usage = json.get("usage");
        let input_tokens = usage
            .and_then(|u| u["input_tokens"].as_u64())
            .unwrap_or(0) as u32;
        let output_tokens = usage
            .and_then(|u| u["output_tokens"].as_u64())
            .unwrap_or(0) as u32;
        let _ = app
            .usage
            .record_llm(
                app.user_id(),
                &request.provider,
                &request.model,
                "writing_chat",
                None,
                LlmUsage {
                    input_tokens,
                    output_tokens,
                },
            )
            .await;
        return Ok(ChatResponse { success: true, reply, error: String::new() });
    }

    // OpenAI-compatible (TokenMix)
    let resp = match client.post(base_url)
        .header("Authorization", format!("Bearer {}", request.api_key))
        .header("content-type", "application/json")
        .json(&body)
        .send().await
    {
        Ok(r) => r,
        Err(e) => return Ok(ChatResponse { success: false, reply: String::new(), error: format!("Request failed: {}", e) }),
    };

    let json: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return Ok(ChatResponse { success: false, reply: String::new(), error: format!("Parse failed: {}", e) }),
    };

    if let Some(err) = json.get("error") {
        return Ok(ChatResponse { success: false, reply: String::new(), error: format!("API: {}", err["message"].as_str().unwrap_or("unknown")) });
    }

    let reply = json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
    let usage = json.get("usage");
    let input_tokens = usage
        .and_then(|u| u["prompt_tokens"].as_u64())
        .unwrap_or(0) as u32;
    let output_tokens = usage
        .and_then(|u| u["completion_tokens"].as_u64())
        .unwrap_or(0) as u32;
    let _ = app
        .usage
        .record_llm(
            app.user_id(),
            &request.provider,
            &request.model,
            "writing_chat",
            None,
            LlmUsage {
                input_tokens,
                output_tokens,
            },
        )
        .await;
    Ok(ChatResponse { success: true, reply, error: String::new() })
}

pub async fn get_ai_spend_totals(app: &AppCtx) -> Result<crate::usage::AiSpendTotals, String> {
    crate::usage::ai_spend_totals(&app.db.pool, app.user_id()).await
}
