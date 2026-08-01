// analysis/content_advisory.rs — Content & maturity advisory for wide distribution.

use std::collections::HashMap;

use crate::app_ctx::AppCtx;
use crate::db;
use crate::prompts;

#[derive(Debug, Default)]
pub struct ContentSignalsSummary {
    pub heat_levels:    Vec<String>,
    pub faith_markets:  Vec<String>,
    pub tropes:         Vec<String>,
    pub chapter_count:  usize,
}

/// Aggregate heat, faith_market, and tropes from per-chapter summary JSON.
pub fn aggregate_content_signals(summaries: &[db::ChapterSummaryRow]) -> ContentSignalsSummary {
    let mut out = ContentSignalsSummary {
        chapter_count: summaries.len(),
        ..Default::default()
    };

    for summary in summaries {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&summary.signals) else {
            continue;
        };
        let obj = value.get("summary").unwrap_or(&value);
        if let Some(heat) = obj.get("heat").and_then(|v| v.as_str()) {
            let h = heat.trim().to_lowercase();
            if !h.is_empty() && !out.heat_levels.iter().any(|x| x == &h) {
                out.heat_levels.push(h);
            }
        }
        if let Some(faith) = obj.get("faith_market").and_then(|v| v.as_str()) {
            let f = faith.trim().to_lowercase();
            if !f.is_empty() && !out.faith_markets.iter().any(|x| x == &f) {
                out.faith_markets.push(f);
            }
        }
        if let Some(tropes) = obj.get("tropes").and_then(|v| v.as_array()) {
            for t in tropes {
                if let Some(s) = t.as_str() {
                    let trope = s.trim().to_string();
                    if !trope.is_empty() && !out.tropes.contains(&trope) {
                        out.tropes.push(trope);
                    }
                }
            }
        }
    }

    out
}

pub async fn generate_content_maturity_advisory(
    app: &AppCtx,
    story_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    genre_data: &db::GenreDataRow,
    signals: &ContentSignalsSummary,
) -> Result<serde_json::Value, String> {
    let heat_summary = if signals.heat_levels.is_empty() {
        "unknown".to_string()
    } else {
        signals.heat_levels.join(", ")
    };
    let faith_summary = if signals.faith_markets.is_empty() {
        "unknown".to_string()
    } else {
        signals.faith_markets.join(", ")
    };
    let tropes_summary = if signals.tropes.is_empty() {
        "none noted".to_string()
    } else {
        signals.tropes.join(", ")
    };

    let chapter_count = signals.chapter_count.to_string();
    let mut vars = HashMap::new();
    vars.insert("industry_ebook", genre_data.industry_ebook.as_str());
    vars.insert("industry_print", genre_data.industry_print.as_str());
    vars.insert("reader_demographic", genre_data.reader_demographic.as_str());
    vars.insert("genre_signals", genre_data.genre_signals.as_str());
    vars.insert("heat_summary", heat_summary.as_str());
    vars.insert("faith_summary", faith_summary.as_str());
    vars.insert("tropes_summary", tropes_summary.as_str());
    vars.insert("chapter_count", chapter_count.as_str());

    let raw = prompts::execute_prompt(
        app,
        "content_maturity_advisory",
        provider,
        api_key,
        model,
        vars,
        Some(story_id),
    )
    .await?;

    let clean = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let mut value: serde_json::Value = serde_json::from_str(clean)
        .map_err(|e| format!("JSON parse: {} | got: {}", e, &clean[..clean.len().min(300)]))?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert("schema".to_string(), serde_json::json!("content_maturity_advisory_v1"));
    }
    Ok(value)
}
