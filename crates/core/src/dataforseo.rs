// dataforseo.rs — DataForSEO API client for keyword volume data.
//
// Base URL: https://api.dataforseo.com/v3/
// Auth: HTTP Basic (login:password)
// Endpoints used:
//   /keywords_data/google_ads/search_volume/live     — Google search volume for keywords
//   /dataforseo_labs/amazon/related_keywords/live    — Amazon related keywords with volume

use base64::Engine;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;

use crate::app_ctx::AppCtx;

const BASE_URL: &str = "https://api.dataforseo.com/v3";
const TIMEOUT_SECS: u64 = 30;

// ── Client ────────────────────────────────────────────────────────────────────

pub struct DataForSeoClient {
    client: Client,
    auth_header: String,
}

impl DataForSeoClient {
    pub fn new(login: &str, password: &str) -> Result<Self, String> {
        let login = login.trim();
        let password = password.trim();
        if login.is_empty() || password.is_empty() {
            return Err(
                "DataForSEO login and API password are not configured. Set them in Admin → Platform credentials.".into(),
            );
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECS))
            .build()
            .map_err(|e| format!("HTTP client error: {}", e))?;

        let credentials = format!("{}:{}", login, password);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());
        let auth_header = format!("Basic {}", encoded);

        Ok(Self { client, auth_header })
    }

    async fn post(&self, path: &str, body: &[Value]) -> Result<Value, String> {
        let url = format!("{}{}", BASE_URL, path);
        let resp = self.client
            .post(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| format!("DataForSEO request failed: {}", e))?;

        let status = resp.status();
        let body_text = resp.text().await.map_err(|e| format!("DataForSEO read error: {}", e))?;

        if !status.is_success() {
            let snippet = &body_text[..body_text.len().min(300)];
            if status.as_u16() == 401 || body_text.contains("40100") {
                return Err(format!(
                    "DataForSEO rejected the API credentials (401). Use the API login and API password from https://app.dataforseo.com/api-access — not your website password. Re-save in Admin → Platform credentials and use Test DataForSEO. Response: {snippet}"
                ));
            }
            return Err(format!("DataForSEO API error ({}): {}", status.as_u16(), snippet));
        }

        let json: Value = serde_json::from_str(&body_text)
            .map_err(|e| format!("DataForSEO JSON parse error: {}", e))?;

        // Check for API-level errors
        let status_code = json["status_code"].as_u64().unwrap_or(0);
        if status_code != 20000 {
            let msg = json["status_message"].as_str().unwrap_or("unknown error");
            return Err(format!("DataForSEO error ({}): {}", status_code, msg));
        }

        Ok(json)
    }
}

// ── Google Ads Search Volume ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct KeywordVolume {
    pub keyword: String,
    pub search_volume: u64,
    pub competition: String,      // "LOW", "MEDIUM", "HIGH"
    pub cpc: f64,
    pub monthly_searches: Vec<MonthlySearch>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonthlySearch {
    pub year: u32,
    pub month: u32,
    pub search_volume: u64,
}

impl DataForSeoClient {
    /// Get Google Ads search volume for up to 1000 keywords at once.
    /// location_code 2840 = United States.
    pub async fn google_search_volume(&self, keywords: &[String]) -> Result<Vec<KeywordVolume>, String> {
        if keywords.is_empty() { return Ok(Vec::new()); }

        let task = serde_json::json!({
            "location_code": 2840,
            "keywords": keywords,
        });

        let resp = self.post("/keywords_data/google_ads/search_volume/live", &[task]).await?;

        let mut results = Vec::new();
        if let Some(tasks) = resp["tasks"].as_array() {
            for task in tasks {
                if let Some(items) = task["result"].as_array() {
                    for item in items {
                        let keyword = item["keyword"].as_str().unwrap_or("").to_string();
                        let search_volume = item["search_volume"].as_u64().unwrap_or(0);
                        let competition = item["competition"].as_str().unwrap_or("").to_string();
                        let cpc = item["cpc"].as_f64().unwrap_or(0.0);

                        let monthly_searches: Vec<MonthlySearch> = item["monthly_searches"]
                            .as_array()
                            .map(|arr| arr.iter().map(|m| MonthlySearch {
                                year: m["year"].as_u64().unwrap_or(0) as u32,
                                month: m["month"].as_u64().unwrap_or(0) as u32,
                                search_volume: m["search_volume"].as_u64().unwrap_or(0),
                            }).collect())
                            .unwrap_or_default();

                        results.push(KeywordVolume {
                            keyword, search_volume, competition, cpc, monthly_searches,
                        });
                    }
                }
            }
        }

        Ok(results)
    }
}

// ── Amazon Related Keywords ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AmazonKeyword {
    pub keyword: String,
    pub search_volume: u64,
}

impl DataForSeoClient {
    /// Get Amazon related keywords with search volume for a seed keyword.
    /// Returns the seed + related keywords.
    pub async fn amazon_related_keywords(&self, seed: &str, limit: u32) -> Result<Vec<AmazonKeyword>, String> {
        let task = serde_json::json!({
            "keyword": seed,
            "language_name": "English",
            "location_code": 2840,
            "limit": limit,
            "include_seed_keyword": true,
        });

        let resp = self.post("/dataforseo_labs/amazon/related_keywords/live", &[task]).await?;

        let mut results = Vec::new();
        if let Some(tasks) = resp["tasks"].as_array() {
            for task in tasks {
                if let Some(items) = task["result"].as_array() {
                    for item in items {
                        // The seed keyword info
                        if let Some(seed_info) = item["seed_keyword_data"].as_object() {
                            if let Some(ki) = seed_info.get("keyword_info") {
                                results.push(AmazonKeyword {
                                    keyword: ki["keyword"].as_str().unwrap_or(seed).to_string(),
                                    search_volume: ki["search_volume"].as_u64().unwrap_or(0),
                                });
                            }
                        }

                        // Related keywords
                        if let Some(related) = item["items"].as_array() {
                            for rel in related {
                                if let Some(ki) = rel["keyword_data"].as_object() {
                                    if let Some(info) = ki.get("keyword_info") {
                                        results.push(AmazonKeyword {
                                            keyword: info["keyword"].as_str().unwrap_or("").to_string(),
                                            search_volume: info["search_volume"].as_u64().unwrap_or(0),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Batch version — sends multiple seeds in one API call (up to 100 tasks per request).
    /// More efficient than calling amazon_related_keywords per seed.
    pub async fn amazon_related_keywords_batch(&self, seeds: &[String], limit: u32) -> Result<Vec<AmazonKeyword>, String> {
        if seeds.is_empty() { return Ok(Vec::new()); }

        // Build array of tasks — one per seed, all in one request
        let tasks: Vec<Value> = seeds.iter().map(|seed| {
            serde_json::json!({
                "keyword": seed,
                "language_name": "English",
                "location_code": 2840,
                "limit": limit,
                "include_seed_keyword": true,
            })
        }).collect();

        let resp = self.post("/dataforseo_labs/amazon/related_keywords/live", &tasks).await?;

        let mut results = Vec::new();
        if let Some(tasks_arr) = resp["tasks"].as_array() {
            for task in tasks_arr {
                if let Some(items) = task["result"].as_array() {
                    for item in items {
                        if let Some(seed_info) = item["seed_keyword_data"].as_object() {
                            if let Some(ki) = seed_info.get("keyword_info") {
                                results.push(AmazonKeyword {
                                    keyword: ki["keyword"].as_str().unwrap_or("").to_string(),
                                    search_volume: ki["search_volume"].as_u64().unwrap_or(0),
                                });
                            }
                        }
                        if let Some(related) = item["items"].as_array() {
                            for rel in related {
                                if let Some(ki) = rel["keyword_data"].as_object() {
                                    if let Some(info) = ki.get("keyword_info") {
                                        results.push(AmazonKeyword {
                                            keyword: info["keyword"].as_str().unwrap_or("").to_string(),
                                            search_volume: info["search_volume"].as_u64().unwrap_or(0),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Simple connection test — try to get volume for one keyword.
    pub async fn test_connection(&self) -> Result<(), String> {
        let keywords = vec!["test".to_string()];
        self.google_search_volume(&keywords).await?;
        Ok(())
    }

    /// Google autocomplete suggestions for seed expansion (wide keyword search).
    pub async fn google_autocomplete_suggestions(
        &self,
        seeds: &[String],
        max_per_seed: u32,
    ) -> Result<Vec<String>, String> {
        let seeds = normalize_keyword_list(seeds);
        if seeds.is_empty() {
            return Ok(Vec::new());
        }

        let tasks: Vec<Value> = seeds
            .iter()
            .map(|seed| {
                serde_json::json!({
                    "keyword": seed,
                    "location_code": 2840,
                    "language_code": "en",
                    "limit": max_per_seed.max(5),
                })
            })
            .collect();

        let resp = self.post("/serp/google/autocomplete/live/advanced", &tasks).await?;
        let mut collected = Vec::new();
        let mut seen = HashSet::new();

        if let Some(tasks_arr) = resp["tasks"].as_array() {
            for task in tasks_arr {
                if let Some(results_arr) = task["result"].as_array() {
                    for result in results_arr {
                        let mut values = Vec::new();
                        collect_string_fields(result, &mut values, &["keyword", "suggestion", "title"]);
                        for value in values {
                            let v = value.trim();
                            if v.is_empty() {
                                continue;
                            }
                            let key = v.to_lowercase();
                            if seen.insert(key) {
                                collected.push(v.to_string());
                            }
                        }
                    }
                }
            }
        }

        Ok(collected)
    }
}

fn collect_string_fields(value: &Value, out: &mut Vec<String>, keys: &[&str]) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                if keys.iter().any(|wanted| wanted.eq_ignore_ascii_case(k)) {
                    if let Some(s) = v.as_str() {
                        let trimmed = s.trim();
                        if !trimmed.is_empty() {
                            out.push(trimmed.to_string());
                        }
                    }
                }
                collect_string_fields(v, out, keys);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_string_fields(item, out, keys);
            }
        }
        _ => {}
    }
}

fn normalize_keyword_list(raw: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for k in raw {
        let trimmed = k.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = trimmed.to_lowercase();
        if seen.insert(normalized) {
            out.push(trimmed.to_string());
        }
    }
    out
}

// ── Tauri commands ────────────────────────────────────────────────────────────


fn emit_dfs(app: &AppCtx, msg: &str) {
    let _ = app.emit("cdp:log", msg);
}

#[derive(serde::Serialize)]
pub struct DfsTestResult {
    pub success: bool,
    pub error: String,
}

pub async fn test_dataforseo_connection(login: String, password: String) -> DfsTestResult {
    let client = match DataForSeoClient::new(&login, &password) {
        Ok(c) => c,
        Err(e) => return DfsTestResult { success: false, error: e },
    };
    match client.test_connection().await {
        Ok(()) => DfsTestResult { success: true, error: String::new() },
        Err(e) => DfsTestResult { success: false, error: e },
    }
}

/// Search for Amazon keywords related to seeds and return volume data.
/// Used by the KDP keyword pipeline.
pub async fn search_amazon_keywords(
    app: AppCtx,
    seeds: Vec<String>,
    login: String,
    password: String,
) -> crate::models::KeywordSearchResponse {
    use crate::models::{KeywordResult, KeywordSearchResponse};

    let client = match DataForSeoClient::new(&login, &password) {
        Ok(c) => c,
        Err(e) => return KeywordSearchResponse { success: false, results: Vec::new(), error: e },
    };

    emit_dfs(&app, &format!("Searching Amazon keywords for {} seed(s)...", seeds.len()));

    let mut all_results: Vec<KeywordResult> = Vec::new();

    for seed in &seeds {
        if crate::is_cancelled() { break; }
        emit_dfs(&app, &format!("  Seed: \"{}\"", seed));

        match client.amazon_related_keywords(seed, 20).await {
            Ok(keywords) => {
                emit_dfs(&app, &format!("    {} keywords found.", keywords.len()));
                for kw in keywords {
                    // Classify competition based on search volume
                    let competition = if kw.search_volume > 50000 { "High" }
                        else if kw.search_volume > 5000 { "Medium" }
                        else { "Low" };

                    all_results.push(KeywordResult {
                        keyword: kw.keyword,
                        searches: format!("{}", kw.search_volume),
                        competition: competition.to_string(),
                        estimated_earnings: String::new(),
                    });
                }
            }
            Err(e) => {
                emit_dfs(&app, &format!("    ⚠ Error: {}", e));
            }
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    emit_dfs(&app, &format!("✓ {} total Amazon keywords.", all_results.len()));
    KeywordSearchResponse { success: true, results: all_results, error: String::new() }
}

/// Get Google search volume for a list of keywords.
/// Used by the Wide distribution pipeline.
pub async fn search_google_keywords(
    app: AppCtx,
    keywords: Vec<String>,
    login: String,
    password: String,
) -> crate::models::KeywordSearchResponse {
    use crate::models::{KeywordResult, KeywordSearchResponse};

    let client = match DataForSeoClient::new(&login, &password) {
        Ok(c) => c,
        Err(e) => return KeywordSearchResponse { success: false, results: Vec::new(), error: e },
    };

    emit_dfs(&app, &format!("Getting Google volume for {} keyword(s)...", keywords.len()));

    match client.google_search_volume(&keywords).await {
        Ok(volumes) => {
            let results: Vec<KeywordResult> = volumes.iter().map(|v| {
                KeywordResult {
                    keyword: v.keyword.clone(),
                    searches: format!("{}", v.search_volume),
                    competition: v.competition.clone(),
                    estimated_earnings: if v.cpc > 0.0 { format!("${:.2} CPC", v.cpc) } else { String::new() },
                }
            }).collect();

            emit_dfs(&app, &format!("✓ {} keywords with volume data.", results.len()));
            KeywordSearchResponse { success: true, results, error: String::new() }
        }
        Err(e) => {
            emit_dfs(&app, &format!("✗ Keyword volume lookup failed: {}", e));
            KeywordSearchResponse { success: false, results: Vec::new(), error: e }
        }
    }
}
