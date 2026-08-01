// analysis/keywords.rs — KDP keyword optimization, search term generation,
// discovery keywords, and Canopy-based keyword search.

use std::collections::{HashMap, HashSet};
use serde::Deserialize;

use super::{emit, err, extract_json_object, GenreResult};
use crate::app_ctx::AppCtx;
use crate::db;
use crate::prompts;
use crate::models::KeywordResult;

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct KeywordRequest {
    #[serde(alias = "folder")]
    pub story_id:   String,
    pub api_key:  String,
    pub model:    String,
    pub provider: String,
}

// ── Tauri commands ───────────────────────────────────────────────────────────

pub async fn generate_search_terms(app: AppCtx, request: KeywordRequest) -> GenreResult {
    let database = app.db.as_ref();
    let genre_data = db::load_genre_data(&database.pool, &request.story_id).await;
    let genre_data = match genre_data {
        Some(d) => d,
        None    => return err("No genre data found. Run Analyze first."),
    };

    emit(&app, "Generating competition search terms...");
    emit(&app, &format!("  Genre: {}", genre_data.industry_ebook));

    match generate_mi_search_terms(
        &app,
        &request.story_id,
        &request.provider,
        &request.api_key,
        &request.model,
        &genre_data,
    )
    .await {
        Err(e) => err(&format!("AI error: {}", e)),
        Ok(keywords) => {
            emit(&app, &format!("  ✓ {} search terms generated:", keywords.len()));
            for kw in &keywords { emit(&app, &format!("    • {}", kw)); }

            let rendered = render_search_terms(&keywords);
            let _ = db::save_mi_search_terms(&database.pool, &request.story_id, &keywords).await;
            let _ = db::save_document(&database.pool, &request.story_id, "mi_search_terms", &rendered).await;

            GenreResult { success: true, report: rendered, error: String::new(), run_ts: String::new() }
        }
    }
}

pub async fn optimize_keywords(app: AppCtx, request: KeywordRequest) -> GenreResult {
    let database = app.db.as_ref();
    let genre_data = db::load_genre_data(&database.pool, &request.story_id).await;
    let genre_data = match genre_data {
        Some(d) => d,
        None    => return err("No genre data found. Run Full Analysis first."),
    };

    emit(&app, "Extracting keyword material...");
    let source_note = if !genre_data.genre_signals.is_empty() {
        "*(Generated from genre analysis.)*"
    } else {
        "*(Generated from genre analysis. Run Analyze Competition for PR-sourced keywords.)*"
    };

    emit(&app, &format!("Asking {} to optimize keywords...", &request.model));

    match call_keyword_optimizer(
        &app,
        &request.story_id,
        &request.provider,
        &request.api_key,
        &request.model,
        &genre_data,
        &genre_data.genre_signals,
    )
    .await {
        Err(e) => err(&format!("AI error: {}", e)),
        Ok((entries, strategy)) => {
            let rendered = render_kdp_keywords(&entries, &strategy, source_note);
            let _ = db::save_kdp_keywords(&database.pool, &request.story_id, &entries, &strategy, source_note).await;
            let _ = db::save_document(&database.pool, &request.story_id, "kdp_keywords", &rendered).await;
            emit(&app, "✓ KDP keywords saved to database.");
            GenreResult { success: true, report: rendered, error: String::new(), run_ts: String::new() }
        }
    }
}

// ── Core logic ───────────────────────────────────────────────────────────────

pub(crate) async fn generate_mi_search_terms(
    app: &AppCtx,
    story_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    genre_data: &db::GenreDataRow,
) -> Result<Vec<String>, String> {
    let kdp_categories = genre_data.kdp_ebook.iter()
        .map(|p| p.split('>').last().unwrap_or(p).trim().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let genre_signals = &genre_data.genre_signals[..genre_data.genre_signals.len().min(500)];

    let mut vars = HashMap::new();
    vars.insert("genre", genre_data.industry_ebook.as_str());
    vars.insert("kdp_categories", kdp_categories.as_str());
    vars.insert("genre_signals", genre_signals);

    let raw = prompts::execute_prompt(
        app,
        "mi_search_terms",
        provider,
        api_key,
        model,
        vars,
        Some(story_id),
    )
    .await?;
    let clean = raw.trim()
        .trim_start_matches("```json").trim_start_matches("```")
        .trim_end_matches("```").trim();

    serde_json::from_str::<Vec<String>>(clean)
        .map_err(|e| format!("Parse error: {} | got: {}", e, &clean[..clean.len().min(200)]))
}

pub(crate) async fn call_keyword_optimizer(
    app: &AppCtx,
    story_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    genre_data: &db::GenreDataRow,
    keywords_text: &str,
) -> Result<(Vec<db::KdpKeywordEntry>, String), String>
{
    let kdp_ebook = genre_data.kdp_ebook.join(", ");
    let kdp_print = genre_data.kdp_print.join(", ");

    let mut vars = HashMap::new();
    vars.insert("industry_ebook", genre_data.industry_ebook.as_str());
    vars.insert("industry_print", genre_data.industry_print.as_str());
    vars.insert("kdp_ebook", kdp_ebook.as_str());
    vars.insert("kdp_print", kdp_print.as_str());
    vars.insert("keywords_text", keywords_text);

    let raw = prompts::execute_prompt(
        app,
        "kdp_keywords",
        provider,
        api_key,
        model,
        vars,
        Some(story_id),
    )
    .await?;
    let clean = extract_json_object(&raw)
        .ok_or_else(|| format!("No JSON object found in response: {}", &raw[..raw.len().min(200)]))?;

    let v: serde_json::Value = serde_json::from_str(&clean)
        .map_err(|e| format!("JSON parse: {} | got: {}", e, &clean[..clean.len().min(400)]))?;

    let keywords = v["keywords"].as_array().ok_or("Missing keywords array")?;
    let strategy = v["strategy"].as_str().unwrap_or("").to_string();

    let entries: Vec<db::KdpKeywordEntry> = keywords.iter().map(|kw| {
        let s = kw["string"].as_str().unwrap_or("").to_string();
        let chars = kw["chars"].as_i64().unwrap_or(s.len() as i64);
        db::KdpKeywordEntry {
            chars: chars.max(s.len() as i64),
            string: s,
            rationale: kw["rationale"].as_str().unwrap_or("").to_string(),
        }
    }).collect();

    Ok((entries, strategy))
}

pub(crate) fn format_keyword_pool_table(pool: &[KeywordResult]) -> String {
    let mut lines = vec![
        "| Keyword | Monthly Searches | Competition | Est. Earnings |".to_string(),
        "|---------|-----------------|-------------|---------------|".to_string(),
    ];
    for r in pool.iter().take(50) {
        lines.push(format!(
            "| {} | {} | {} | {} |",
            r.keyword, r.searches, r.competition, r.estimated_earnings
        ));
    }
    lines.join("\n")
}

pub(crate) async fn call_keyword_optimizer_with_pool(
    app: &AppCtx,
    story_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    genre_data: &db::GenreDataRow,
    keywords_text: &str,
    keyword_pool: &[KeywordResult],
) -> Result<(Vec<db::KdpKeywordEntry>, String), String> {
    if keyword_pool.is_empty() {
        return call_keyword_optimizer(
            app,
            story_id,
            provider,
            api_key,
            model,
            genre_data,
            keywords_text,
        )
        .await;
    }

    let pool_table = format_keyword_pool_table(keyword_pool);
    let kdp_ebook = genre_data.kdp_ebook.join(", ");
    let kdp_print = genre_data.kdp_print.join(", ");

    let mut vars = HashMap::new();
    vars.insert("industry_ebook", genre_data.industry_ebook.as_str());
    vars.insert("industry_print", genre_data.industry_print.as_str());
    vars.insert("kdp_ebook", kdp_ebook.as_str());
    vars.insert("kdp_print", kdp_print.as_str());
    vars.insert("keywords_text", keywords_text);
    vars.insert("pool_table", pool_table.as_str());

    let raw = prompts::execute_prompt(
        app,
        "kdp_keywords_with_pool",
        provider,
        api_key,
        model,
        vars,
        Some(story_id),
    )
    .await?;
    let clean = extract_json_object(&raw)
        .ok_or_else(|| format!("No JSON object found in response: {}", &raw[..raw.len().min(200)]))?;

    let v: serde_json::Value = serde_json::from_str(&clean)
        .map_err(|e| format!("JSON parse: {} | got: {}", e, &clean[..clean.len().min(400)]))?;

    let keywords = v["keywords"].as_array().ok_or("Missing keywords array")?;
    let strategy = v["strategy"].as_str().unwrap_or("").to_string();

    let entries: Vec<db::KdpKeywordEntry> = keywords.iter().map(|kw| {
        let s = kw["string"].as_str().unwrap_or("").to_string();
        let chars = kw["chars"].as_i64().unwrap_or(s.len() as i64);
        db::KdpKeywordEntry {
            chars: chars.max(s.len() as i64),
            string: s,
            rationale: kw["rationale"].as_str().unwrap_or("").to_string(),
        }
    }).collect();

    Ok((entries, strategy))
}

/// Generate 10 discovery keyword phrases for non-Amazon platforms.
pub(crate) async fn generate_discovery_keywords(
    app: &AppCtx,
    story_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    genre_data: &db::GenreDataRow,
) -> Result<Vec<db::DiscoveryKeywordEntry>, String> {
    let mut vars = HashMap::new();
    vars.insert("industry_ebook", genre_data.industry_ebook.as_str());
    vars.insert("industry_print", genre_data.industry_print.as_str());
    vars.insert("reader_demographic", genre_data.reader_demographic.as_str());
    vars.insert("bookstore_shelving", genre_data.bookstore_shelving.as_str());
    vars.insert("genre_signals", genre_data.genre_signals.as_str());

    let raw = prompts::execute_prompt(
        app,
        "discovery_keywords",
        provider,
        api_key,
        model,
        vars,
        Some(story_id),
    )
    .await?;
    let clean = extract_json_object(&raw)
        .ok_or_else(|| format!("No JSON in discovery response: {}", &raw[..raw.len().min(200)]))?;
    let v: serde_json::Value = serde_json::from_str(&clean)
        .map_err(|e| format!("JSON parse: {} | got: {}", e, &clean[..clean.len().min(400)]))?;

    let keywords = v["keywords"].as_array()
        .ok_or_else(|| "Missing keywords array in discovery response".to_string())?;

    let entries: Vec<db::DiscoveryKeywordEntry> = keywords.iter().map(|kw| {
        db::DiscoveryKeywordEntry {
            phrase:    kw["phrase"].as_str().unwrap_or("").to_string(),
            rationale: kw["rationale"].as_str().unwrap_or("AI-reasoned").to_string(),
        }
    }).collect();

    Ok(entries)
}

/// Derive 2-3 seed terms from existing analysis data.
/// Returns a deduplicated Vec of seed strings (case-insensitive dedup).
pub(crate) fn derive_keyword_seeds(
    industry_ebook: &str,
    top_categories: &[String],
) -> Vec<String> {
    let mut seeds: Vec<String> = Vec::new();

    // Seed 1: first 3 words of industry_ebook, lowercased
    let words: Vec<&str> = industry_ebook.split_whitespace().collect();
    let genre_seed = words[..words.len().min(3)].join(" ").to_lowercase();
    if !genre_seed.is_empty() {
        seeds.push(genre_seed);
    }

    // Seeds 2-3: leaf segment (text after last ">") from top 1-2 category paths
    for cat_path in top_categories.iter().take(2) {
        let leaf = cat_path
            .rsplit('>')
            .next()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        if !leaf.is_empty() {
            seeds.push(leaf);
        }
    }

    // Deduplicate (case-insensitive)
    let mut seen: Vec<String> = Vec::new();
    seeds.retain(|s| {
        let lower = s.to_lowercase();
        if seen.contains(&lower) {
            false
        } else {
            seen.push(lower);
            true
        }
    });

    seeds
}

/// Canopy-based keyword search — replaces legacy keyword search in the pipeline.
/// For each seed: gets autocomplete suggestions, searches top results, estimates volume.
pub(crate) async fn run_keyword_searches_canopy(
    app: &AppCtx,
    folder: &str,
    seeds: &[String],
    canopy_api_key: &str,
) -> Vec<KeywordResult> {
    let mut all_results: Vec<KeywordResult> = Vec::new();

    let client = match crate::canopy::CanopyClient::new(canopy_api_key) {
        Ok(c) => c,
        Err(e) => {
            let _ = app.emit("cdp:log", &format!("⚠ Could not create Canopy client: {}", e));
            return all_results;
        }
    };

    for seed in seeds {
        if crate::is_cancelled() { break; }

        let _ = app.emit("cdp:log", &format!("Keyword search (Canopy): \"{}\"", seed));

        // Get suggestions
        let suggestions = client.autocomplete(seed, "US", Some("digital-text")).await
            .unwrap_or_else(|_| vec![seed.clone()]);
        let mut keywords: Vec<String> = vec![seed.clone()];
        for s in suggestions.into_iter().take(10) {
            if !keywords.contains(&s) { keywords.push(s); }
        }

        let mut results: Vec<KeywordResult> = Vec::new();
        for kw in &keywords {
            let search = match client.search(kw, "US", Some("digital-text"), 1).await {
                Ok(r) => r,
                Err(_) => continue,
            };
            if search.is_empty() {
                results.push(KeywordResult { keyword: kw.clone(), searches: "0".to_string(), competition: "Low".to_string(), estimated_earnings: "$0".to_string() });
                continue;
            }
            let organic: Vec<_> = search.iter().filter(|r| !r.is_sponsored).take(3).collect();
            let mut daily_sales: Vec<f64> = Vec::new();
            for sr in &organic {
                if sr.asin.is_empty() { continue; }
                if let Ok(s) = client.get_sales(&sr.asin, "US").await {
                    if let Some(d) = s.estimated_daily_sales { daily_sales.push(d); }
                }
                tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            }
            let avg = if daily_sales.is_empty() { 0.0 } else { daily_sales.iter().sum::<f64>() / daily_sales.len() as f64 };
            let monthly_searches = (avg * 30.0 * 33.0) as u64;
            let avg_reviews: f64 = {
                let counts: Vec<f64> = search.iter().filter_map(|r| r.review_count.map(|c| c as f64)).collect();
                if counts.is_empty() { 0.0 } else { counts.iter().sum::<f64>() / counts.len() as f64 }
            };
            let sponsored_count = search.iter().filter(|r| r.is_sponsored).count();
            let competition = if avg_reviews > 500.0 || sponsored_count > 5 { "High" }
                else if avg_reviews > 100.0 || sponsored_count > 2 { "Medium" }
                else { "Low" };
            let est_earnings = avg * 30.0 * 0.3 * 2.80;
            results.push(KeywordResult {
                keyword: kw.clone(),
                searches: format!("{}", monthly_searches),
                competition: competition.to_string(),
                estimated_earnings: format!("${:.0}", est_earnings),
            });
        }

        let database = app.db.as_ref();
        let rows: Vec<(String, String, String, String)> = results.iter()
            .map(|r| (r.keyword.clone(), r.searches.clone(), r.competition.clone(), r.estimated_earnings.clone()))
            .collect();
        let _ = crate::db::replace_keyword_search_results(&database.pool, folder, seed, &rows).await;
        let _ = app.emit("cdp:log", &format!("✓ \"{}\" → {} keyword(s).", seed, results.len()));
        all_results.extend(results);
    }

    all_results
}

// ── Rendering ────────────────────────────────────────────────────────────────

pub(crate) fn render_kdp_keywords(entries: &[db::KdpKeywordEntry], strategy: &str, source_note: &str) -> String {
    let json = serde_json::json!({
        "schema": "kdp_keywords_v1",
        "source_note": source_note,
        "entries": entries.iter().enumerate().map(|(i, kw)| {
            serde_json::json!({
                "field": i + 1,
                "string": kw.string,
                "chars": kw.chars,
                "rationale": kw.rationale,
                "over_limit": kw.string.len() > 50,
            })
        }).collect::<Vec<_>>(),
        "strategy": strategy,
    });
    json.to_string()
}

pub(crate) fn render_search_terms(keywords: &[String]) -> String {
    let json = serde_json::json!({
        "schema": "mi_search_terms_v1",
        "keywords": keywords,
    });
    json.to_string()
}

fn normalize_keyword(input: &str) -> String {
    input.trim().to_lowercase()
}

fn classify_competition(search_volume: u64) -> &'static str {
    if search_volume > 50_000 {
        "High"
    } else if search_volume > 5_000 {
        "Medium"
    } else {
        "Low"
    }
}

fn extract_asin_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }
    for marker in ["/dp/", "/gp/product/"] {
        if let Some(idx) = trimmed.find(marker) {
            let rest = &trimmed[idx + marker.len()..];
            let asin: String = rest.chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if asin.len() >= 8 {
                return Some(asin);
            }
        }
    }
    None
}

/// Read competitor ASINs out of the stored competition document, falling back to
/// parsing them out of Amazon URLs when the ASIN field is absent.
async fn load_competitor_asins(app: &AppCtx, story_id: &str) -> Vec<String> {
    let database = app.db.as_ref();
    let Some(raw) = crate::db::get_document(&database.pool, story_id, "competition_data").await else {
        return Vec::new();
    };

    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    if let Some(books) = json["books"].as_array() {
        for book in books {
            if let Some(asin) = book["asin"].as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if seen.insert(asin.to_string()) {
                    out.push(asin.to_string());
                }
                continue;
            }
            if let Some(url) = book["amazon_url"].as_str() {
                if let Some(asin) = extract_asin_from_url(url) {
                    if seen.insert(asin.clone()) {
                        out.push(asin);
                    }
                }
            }
        }
    }
    out
}

/// Merge one keyword observation into the candidate map, keeping the highest
/// volume seen, the shortest surface form, and the set of contributing sources.
fn upsert_volume(
    map: &mut HashMap<String, (String, u64, HashSet<String>)>,
    keyword: &str,
    search_volume: u64,
    source: &str,
) {
    let trimmed = keyword.trim();
    if trimmed.is_empty() {
        return;
    }
    let key = normalize_keyword(trimmed);
    let entry = map
        .entry(key)
        .or_insert_with(|| (trimmed.to_string(), search_volume, HashSet::new()));
    if search_volume > entry.1 {
        entry.1 = search_volume;
    }
    if entry.0.len() > trimmed.len() {
        entry.0 = trimmed.to_string();
    }
    entry.2.insert(source.to_string());
}

async fn record_dfs_usage(app: &AppCtx, story_id: &str, feature: &str) {
    let _ = app
        .usage
        .record_external(
            app.user_id(),
            "dataforseo",
            "dataforseo",
            feature,
            Some(story_id),
        )
        .await;
}

/// DataForSEO-based Amazon keyword research: autocomplete expansion, related
/// keywords, competitor-ASIN ranked keywords and intersections, bulk volume
/// normalization, then trend enrichment over the top candidates.
pub(crate) async fn run_keyword_searches_dataforseo(
    app: &AppCtx,
    story_id: &str,
    seeds: &[String],
    dataforseo_login: &str,
    dataforseo_password: &str,
) -> Vec<KeywordResult> {
    let client = match crate::dataforseo::DataForSeoClient::new(dataforseo_login, dataforseo_password) {
        Ok(c) => c,
        Err(e) => {
            let _ = app.emit("cdp:log", &format!("⚠ Keyword search API error: {}", e));
            return Vec::new();
        }
    };

    let mut candidates: HashMap<String, (String, u64, HashSet<String>)> = HashMap::new();
    for seed in seeds {
        upsert_volume(&mut candidates, seed, 0, "seed");
    }

    let _ = app.emit("cdp:log", &format!("Searching Amazon keywords for {} seed(s)...", seeds.len()));
    for seed in seeds {
        let _ = app.emit("cdp:log", &format!("  Seed: \"{}\"", seed));
    }

    // 1) Google autocomplete expansion for long-tail seed discovery.
    match client.google_autocomplete_suggestions(seeds, 12).await {
        Ok(suggestions) => {
            let _ = app.emit("cdp:log", &format!("  ✓ {} autocomplete suggestions.", suggestions.len()));
            record_dfs_usage(app, story_id, "google_autocomplete_suggestions").await;
            for s in suggestions {
                upsert_volume(&mut candidates, &s, 0, "autocomplete");
            }
        }
        Err(e) => {
            let _ = app.emit("cdp:log", &format!("  ⚠ Autocomplete expansion failed: {}", e));
        }
    }

    // 2) Amazon related keywords from seed set.
    match client.amazon_related_keywords_batch(seeds, 30).await {
        Ok(keywords) => {
            let _ = app.emit("cdp:log", &format!("  ✓ {} related keywords from Amazon endpoint.", keywords.len()));
            record_dfs_usage(app, story_id, "amazon_related_keywords_batch").await;
            for kw in keywords {
                upsert_volume(&mut candidates, &kw.keyword, kw.search_volume, "related");
            }
        }
        Err(e) => {
            let _ = app.emit("cdp:log", &format!("  ⚠ Related-keywords call failed: {}", e));
        }
    }

    // 3) ASIN-driven keyword intelligence (ranked keywords + competitor overlap).
    let mut asins = load_competitor_asins(app, story_id).await;
    if !asins.is_empty() {
        let _ = app.emit("cdp:log", &format!("  Found {} competitor ASINs from competition data.", asins.len()));
    }

    // Expand ASIN set using product-competitor endpoint.
    let base_asins: Vec<String> = asins.iter().take(3).cloned().collect();
    for asin in &base_asins {
        match client.amazon_product_competitors(asin, 10).await {
            Ok(competitors) => {
                record_dfs_usage(app, story_id, "amazon_product_competitors").await;
                for comp in competitors {
                    if !asins.iter().any(|a| a.eq_ignore_ascii_case(&comp)) {
                        asins.push(comp);
                    }
                }
            }
            Err(e) => {
                let _ = app.emit("cdp:log", &format!("  ⚠ Product competitors failed for {}: {}", asin, e));
            }
        }
    }

    // Pull ranked keywords for a few strongest ASIN signals.
    for asin in asins.iter().take(6) {
        match client.amazon_ranked_keywords(asin, 30).await {
            Ok(keywords) => {
                record_dfs_usage(app, story_id, "amazon_ranked_keywords").await;
                for kw in keywords {
                    upsert_volume(&mut candidates, &kw.keyword, kw.search_volume, "ranked");
                }
            }
            Err(e) => {
                let _ = app.emit("cdp:log", &format!("  ⚠ Ranked keywords failed for {}: {}", asin, e));
            }
        }
    }

    // Use intersections for higher-intent overlap terms.
    let intersection_asins: Vec<String> = asins.iter().take(3).cloned().collect();
    if intersection_asins.len() >= 2 {
        match client.amazon_product_keyword_intersections(&intersection_asins, 40).await {
            Ok(keywords) => {
                let _ = app.emit("cdp:log", &format!("  ✓ {} keyword intersections found.", keywords.len()));
                record_dfs_usage(app, story_id, "amazon_product_keyword_intersections").await;
                for kw in keywords {
                    upsert_volume(&mut candidates, &kw.keyword, kw.search_volume, "intersection");
                }
            }
            Err(e) => {
                let _ = app.emit("cdp:log", &format!("  ⚠ Keyword intersections failed: {}", e));
            }
        }
    }

    // 4) Bulk Amazon volume normalization for the unified keyword list.
    let unified_keywords: Vec<String> = candidates.values().map(|v| v.0.clone()).collect();
    match client.amazon_bulk_search_volume(&unified_keywords).await {
        Ok(volumes) => {
            let _ = app.emit("cdp:log", &format!("  ✓ Bulk volume normalized for {} keywords.", volumes.len()));
            record_dfs_usage(app, story_id, "amazon_bulk_search_volume").await;
            for volume in volumes {
                upsert_volume(&mut candidates, &volume.keyword, volume.search_volume, "bulk_volume");
            }
        }
        Err(e) => {
            let _ = app.emit("cdp:log", &format!("  ⚠ Bulk search volume failed: {}", e));
        }
    }

    // 5) Trends signal enrichment for top candidates.
    let mut ranked_for_trends: Vec<(String, u64)> = candidates
        .values()
        .map(|(k, v, _)| (k.clone(), *v))
        .collect();
    ranked_for_trends.sort_by(|a, b| b.1.cmp(&a.1));
    let trend_targets: Vec<String> = ranked_for_trends.iter().take(60).map(|(k, _)| k.clone()).collect();

    let mut trend_map: HashMap<String, f64> = HashMap::new();
    if !trend_targets.is_empty() {
        match client.dataforseo_trends_delta(&trend_targets).await {
            Ok(trends) => {
                let _ = app.emit("cdp:log", &format!("  ✓ Trend deltas fetched for {} keywords.", trends.len()));
                record_dfs_usage(app, story_id, "dataforseo_trends_delta").await;
                for t in trends {
                    trend_map.insert(normalize_keyword(&t.keyword), t.trend_delta);
                }
            }
            Err(e) => {
                let _ = app.emit("cdp:log", &format!("  ⚠ Trend lookup failed: {}", e));
            }
        }
    }

    let mut all_results: Vec<KeywordResult> = candidates
        .into_iter()
        .map(|(key, (keyword, volume, sources))| {
            let mut source_tags: Vec<String> = sources.into_iter().collect();
            source_tags.sort();
            let trend_suffix = trend_map
                .get(&key)
                .map(|d| format!("trend {:+.0}%", d))
                .unwrap_or_default();
            let provenance = if trend_suffix.is_empty() {
                source_tags.join(",")
            } else {
                format!("{} | {}", trend_suffix, source_tags.join(","))
            };

            KeywordResult {
                keyword,
                searches: volume.to_string(),
                competition: classify_competition(volume).to_string(),
                estimated_earnings: provenance,
            }
        })
        .collect();

    all_results.sort_by(|a, b| {
        let av = a.searches.parse::<u64>().unwrap_or(0);
        let bv = b.searches.parse::<u64>().unwrap_or(0);
        bv.cmp(&av).then_with(|| a.keyword.cmp(&b.keyword))
    });

    if all_results.len() > 250 {
        all_results.truncate(250);
    }

    // Persist results to DB
    if !all_results.is_empty() {
        let database = app.db.as_ref();
        let rows: Vec<(String, String, String, String)> = all_results.iter()
            .map(|r| (r.keyword.clone(), r.searches.clone(), r.competition.clone(), r.estimated_earnings.clone()))
            .collect();
        if let Some(first_seed) = seeds.first() {
            let _ = crate::db::replace_keyword_search_results(&database.pool, story_id, first_seed, &rows).await;
        }
    }
    let _ = app.emit("cdp:log", &format!("✓ {} total enriched Amazon keywords.", all_results.len()));
    all_results
}

/// Derive Google SEO seeds from genre label + discovery phrases (wide distribution).
pub(crate) fn derive_wide_keyword_seeds(
    industry_ebook: &str,
    discovery_phrases: &[String],
) -> Vec<String> {
    let mut seeds = derive_keyword_seeds(industry_ebook, &[]);
    for phrase in discovery_phrases.iter().take(5) {
        let p = phrase.trim().to_lowercase();
        if !p.is_empty() && !seeds.iter().any(|s| s.eq_ignore_ascii_case(&p)) {
            seeds.push(p);
        }
    }
    seeds
}

/// Google search volume research for wide-store SEO (DataForSEO only).
pub(crate) async fn run_google_keyword_searches_dataforseo(
    app: &AppCtx,
    story_id: &str,
    seeds: &[String],
    dataforseo_login: &str,
    dataforseo_password: &str,
) -> Vec<KeywordResult> {
    if seeds.is_empty() {
        return Vec::new();
    }

    let client = match crate::dataforseo::DataForSeoClient::new(dataforseo_login, dataforseo_password) {
        Ok(c) => c,
        Err(e) => {
            let _ = app.emit("cdp:log", &format!("⚠ DataForSEO client error: {}", e));
            return Vec::new();
        }
    };

    let mut candidates: HashSet<String> = HashSet::new();
    for seed in seeds {
        candidates.insert(seed.trim().to_lowercase());
    }

    let _ = app.emit("cdp:log", &format!("DataForSEO: Expanding {} Google seed(s)...", seeds.len()));
    if let Ok(suggestions) = client.google_autocomplete_suggestions(seeds, 15).await {
        let _ = app.emit("cdp:log", &format!("  ✓ {} Google autocomplete suggestions.", suggestions.len()));
        let _ = app
            .usage
            .record_external(
                app.user_id(),
                "dataforseo",
                "dataforseo",
                "google_autocomplete_suggestions",
                Some(story_id),
            )
            .await;
        for s in suggestions {
            let t = s.trim().to_lowercase();
            if !t.is_empty() {
                candidates.insert(t);
            }
        }
    }

    let keywords: Vec<String> = candidates.into_iter().take(80).collect();
    if keywords.is_empty() {
        return Vec::new();
    }

    let _ = app.emit("cdp:log", &format!("DataForSEO: Getting Google volume for {} keyword(s)...", keywords.len()));
    let volumes = match client.google_search_volume(&keywords).await {
        Ok(v) => {
            let _ = app
                .usage
                .record_external(
                    app.user_id(),
                    "dataforseo",
                    "dataforseo",
                    "google_search_volume",
                    Some(story_id),
                )
                .await;
            v
        }
        Err(e) => {
            let _ = app.emit("cdp:log", &format!("  ⚠ Google volume lookup failed: {}", e));
            return Vec::new();
        }
    };

    let mut all_results: Vec<KeywordResult> = volumes
        .into_iter()
        .map(|v| KeywordResult {
            keyword: v.keyword,
            searches: format!("{}", v.search_volume),
            competition: v.competition,
            estimated_earnings: if v.cpc > 0.0 {
                format!("${:.2} CPC", v.cpc)
            } else {
                String::new()
            },
        })
        .collect();

    all_results.sort_by(|a, b| {
        let av = a.searches.parse::<u64>().unwrap_or(0);
        let bv = b.searches.parse::<u64>().unwrap_or(0);
        bv.cmp(&av).then_with(|| a.keyword.cmp(&b.keyword))
    });

    // Google rows live in their own table so Amazon keyword rows stay distinct.
    if !all_results.is_empty() {
        let database = app.db.as_ref();
        let rows: Vec<(String, String, String, String)> = all_results
            .iter()
            .map(|r| {
                (
                    r.keyword.clone(),
                    r.searches.clone(),
                    r.competition.clone(),
                    r.estimated_earnings.clone(),
                )
            })
            .collect();
        let _ = crate::db::replace_google_keyword_search_results(&database.pool, story_id, &rows).await;
        // Drop rows from when Google results shared the Amazon keyword table under
        // a reserved seed — they would otherwise count as Amazon keyword results.
        let _ = crate::db::replace_keyword_search_results(&database.pool, story_id, "__google__", &[]).await;
    }

    let _ = app.emit(
        "cdp:log",
        &format!("✓ DataForSEO: {} Google keywords with volume data.", all_results.len()),
    );
    all_results
}
