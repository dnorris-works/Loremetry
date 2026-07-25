// analysis/tropes.rs — Trope Extraction & Market Fit (Shared: KDP + Wide).
//
// Pass 1 (AI): extract tropes from chapter summaries.
// Pass 2 (non-AI): cross-reference against seeded trope_config + story genre
// context. Market notes are taxonomy-based — never fabricated demand stats.

use std::collections::{HashMap, HashSet};
use tauri::AppHandle;

use super::chapters::build_combined_context;
use super::{emit, err, extract_json_object, GenreResult};
use crate::db::{self, TropeCatalogEntry};
use crate::prompts;

#[derive(Clone, Debug)]
struct ExtractedTrope {
    name: String,
    confidence: u8,
    evidence: Vec<String>,
}

/// Run trope extraction + taxonomy market-fit for a story folder.
/// Requires chapter summaries; uses genre_data / genre_rankings when present.
pub async fn run_trope_market_fit(
    app: &AppHandle,
    database: &db::Db,
    folder: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    run_ts: &str,
) -> GenreResult {
    if api_key.is_empty() || model.is_empty() {
        return err("Trope Extraction & Market Fit requires an API key and model.");
    }

    let (summaries, genre_context, target_genres, catalog) = {
        let conn = database.0.lock().unwrap();
        let summaries = db::load_chapter_summaries(&conn, folder);
        let genre_data = db::load_genre_data(&conn, folder);
        let rankings_rows = db::get_genre_rankings(&conn, folder, "Kindle").unwrap_or_default();
        let rankings: Vec<(String, u8)> = rankings_rows
            .iter()
            .map(|r| (r.genre.clone(), r.confidence.clamp(0, 100) as u8))
            .collect();
        let catalog = db::load_trope_config(&conn).catalog;

        let mut target_genres: Vec<String> = Vec::new();
        if let Some(ref g) = genre_data {
            if !g.industry_ebook.trim().is_empty() {
                target_genres.push(g.industry_ebook.clone());
            }
            if !g.industry_print.trim().is_empty()
                && !g.industry_print.eq_ignore_ascii_case(&g.industry_ebook)
            {
                target_genres.push(g.industry_print.clone());
            }
        }
        for (genre, conf) in &rankings {
            if *conf >= 40 && !target_genres.iter().any(|t| t.eq_ignore_ascii_case(genre)) {
                target_genres.push(genre.clone());
            }
        }

        let genre_context = match &genre_data {
            Some(g) => format!(
                "Industry ebook: {}\nIndustry print: {}\nGenre signals: {}\nTop ranked genres: {}",
                g.industry_ebook,
                g.industry_print,
                g.genre_signals,
                rankings.iter().take(5).map(|(n, c)| format!("{} ({}%)", n, c)).collect::<Vec<_>>().join("; ")
            ),
            None => {
                if rankings.is_empty() {
                    "(No genre analysis on file.)".to_string()
                } else {
                    format!(
                        "Top ranked genres: {}",
                        rankings.iter().take(5).map(|(n, c)| format!("{} ({}%)", n, c)).collect::<Vec<_>>().join("; ")
                    )
                }
            }
        };

        (summaries, genre_context, target_genres, catalog)
    };

    if summaries.is_empty() {
        return err("Trope Extraction needs Chapter Summaries first.");
    }

    // ── Pass 1: AI extraction ─────────────────────────────────────────────
    emit(app, "Trope Extraction: extracting tropes from chapter summaries...");
    let catalog_hint: String = catalog
        .iter()
        .map(|t| {
            if t.aliases.is_empty() {
                t.name.clone()
            } else {
                format!("{} (aka {})", t.name, t.aliases.join(", "))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let combined = build_combined_context(&summaries);
    let mut vars = HashMap::new();
    vars.insert("catalog", catalog_hint.as_str());
    vars.insert("genre_context", genre_context.as_str());
    vars.insert("combined", combined.as_str());

    let extracted = match prompts::execute_prompt(database, "trope_extract", provider, api_key, model, vars).await {
        Ok(raw) => match parse_extracted_tropes(&raw) {
            Ok(t) => t,
            Err(e) => return err(&e),
        },
        Err(e) => return err(&e),
    };

    if crate::is_cancelled() {
        return err("Cancelled.");
    }

    // ── Pass 2: non-AI taxonomy crosswalk ─────────────────────────────────
    emit(app, "Trope Extraction: cross-referencing trope catalog (taxonomy — not measured demand)...");
    let report = build_market_fit_report(&extracted, &catalog, &target_genres);
    {
        let conn = database.0.lock().unwrap();
        let _ = db::save_document_at(&conn, folder, "trope_market_fit", &report, run_ts);
    }

    let n = extracted.len();
    emit(app, &format!("✓ Trope Extraction & Market Fit — {} trope(s).", n));
    GenreResult {
        success: true,
        report,
        error: String::new(),
        run_ts: run_ts.to_string(),
    }
}

fn parse_extracted_tropes(raw: &str) -> Result<Vec<ExtractedTrope>, String> {
    let clean = extract_json_object(raw).ok_or_else(|| "No JSON in trope extraction response.".to_string())?;
    let parsed: serde_json::Value =
        serde_json::from_str(&clean).map_err(|e| format!("JSON parse: {}", e))?;

    let arr = parsed
        .get("tropes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Missing tropes array.".to_string())?;

    let mut tropes: Vec<ExtractedTrope> = Vec::new();
    for item in arr {
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        let confidence = item
            .get("confidence")
            .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|i| i as u64)))
            .unwrap_or(0)
            .min(100) as u8;
        let evidence: Vec<String> = item
            .get("evidence")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        tropes.push(ExtractedTrope {
            name,
            confidence,
            evidence,
        });
    }
    tropes.sort_by(|a, b| b.confidence.cmp(&a.confidence).then_with(|| a.name.cmp(&b.name)));
    Ok(tropes)
}

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn match_catalog<'a>(name: &str, catalog: &'a [TropeCatalogEntry]) -> Option<&'a TropeCatalogEntry> {
    let n = norm(name);
    for entry in catalog {
        if norm(&entry.name) == n || norm(&entry.id.replace('_', " ")) == n {
            return Some(entry);
        }
        for alias in &entry.aliases {
            if norm(alias) == n {
                return Some(entry);
            }
        }
    }
    // Soft contains match (prefer exact above)
    for entry in catalog {
        let en = norm(&entry.name);
        if n.contains(&en) || en.contains(&n) {
            return Some(entry);
        }
    }
    None
}

fn genre_overlap(entry: &TropeCatalogEntry, target_genres: &[String]) -> Vec<String> {
    let mut hits = Vec::new();
    for g in target_genres {
        let gn = norm(g);
        for cg in &entry.common_in_genres {
            let cn = norm(cg);
            if gn == cn || gn.contains(&cn) || cn.contains(&gn) {
                if !hits.iter().any(|h: &String| h.eq_ignore_ascii_case(cg)) {
                    hits.push(cg.clone());
                }
            }
        }
    }
    hits
}

fn build_market_fit_report(
    extracted: &[ExtractedTrope],
    catalog: &[TropeCatalogEntry],
    target_genres: &[String],
) -> String {
    let mut matched_ids: HashSet<String> = HashSet::new();
    let mut trope_rows: Vec<serde_json::Value> = Vec::new();

    for t in extracted {
        let catalog_match = match_catalog(&t.name, catalog);
        if let Some(entry) = catalog_match {
            matched_ids.insert(entry.id.clone());
        }

        let (fit_label, market_note) = match catalog_match {
            Some(entry) => {
                let overlap = genre_overlap(entry, target_genres);
                if overlap.is_empty() {
                    (
                        "catalog_match_off_target",
                        format!(
                            "Matched catalog trope \"{}\". Not listed as common for this story's target genre(s) ({}). Taxonomy expectation only — AI-reasoned, not measured.",
                            entry.name,
                            if target_genres.is_empty() {
                                "unknown".to_string()
                            } else {
                                target_genres.join(", ")
                            }
                        ),
                    )
                } else {
                    (
                        "well_represented_for_category",
                        format!(
                            "Matched catalog trope \"{}\", common in taxonomy for: {}. Well-represented relative to those genre expectations — AI-reasoned, not measured (no live trope-demand data).",
                            entry.name,
                            overlap.join(", ")
                        ),
                    )
                }
            }
            None => (
                "uncatalogued",
                "Detected from manuscript signals but not in the seeded trope catalog. No taxonomy market crosswalk — AI-reasoned, not measured.".to_string(),
            ),
        };

        trope_rows.push(serde_json::json!({
            "name": t.name,
            "catalog_id": catalog_match.map(|e| e.id.clone()),
            "catalog_name": catalog_match.map(|e| e.name.clone()),
            "confidence": t.confidence,
            "evidence": t.evidence,
            "fit_label": fit_label,
            "market_note": market_note,
        }));
    }

    // Expected-but-thin: catalog tropes common in target genres but not detected
    let mut thin: Vec<serde_json::Value> = Vec::new();
    if !target_genres.is_empty() {
        for entry in catalog {
            if matched_ids.contains(&entry.id) {
                continue;
            }
            let overlap = genre_overlap(entry, target_genres);
            if overlap.is_empty() {
                continue;
            }
            // Only flag when strongly associated (appears in at least one overlapping genre)
            thin.push(serde_json::json!({
                "name": entry.name,
                "catalog_id": entry.id,
                "expected_in_genres": overlap,
                "note": format!(
                    "Common in taxonomy for {} but not clearly detected in chapter summaries — thin relative to category expectations. AI-reasoned, not measured.",
                    overlap.join(", ")
                ),
            }));
        }
    }
    // Cap thin list so the report stays readable
    thin.truncate(12);

    let strong: Vec<&str> = extracted
        .iter()
        .filter(|t| t.confidence >= 60)
        .take(3)
        .map(|t| t.name.as_str())
        .collect();
    let thin_names: Vec<&str> = thin
        .iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
        .take(3)
        .collect();

    let coverage_summary = {
        let mut parts = Vec::new();
        if !strong.is_empty() {
            parts.push(format!("leans heavily on {}", strong.join(" and ")));
        } else if !extracted.is_empty() {
            parts.push(format!(
                "shows moderate signals for {}",
                extracted.iter().take(2).map(|t| t.name.as_str()).collect::<Vec<_>>().join(" and ")
            ));
        } else {
            parts.push("shows few clear trope signals in the chapter summaries".to_string());
        }
        if !thin_names.is_empty() {
            parts.push(format!(
                "thin on {} which is common in this category's taxonomy",
                thin_names.join(" and ")
            ));
        }
        if target_genres.is_empty() {
            parts.push("target genre unclear — run Genre Analysis/Ranking for sharper fit notes".to_string());
        }
        let mut s = parts.join("; ");
        if !s.is_empty() {
            let mut chars = s.chars();
            if let Some(c) = chars.next() {
                s = c.to_uppercase().collect::<String>() + chars.as_str();
            }
        }
        s.push('.');
        s
    };

    serde_json::json!({
        "schema": "trope_market_fit_v1",
        "market_data_basis": "taxonomy_only",
        "market_disclaimer": "AI-reasoned, not measured — this app has no live trope-demand dataset. Fit notes compare extracted tropes to the seeded trope catalog and genre affinity lists only.",
        "target_genres": target_genres,
        "tropes": trope_rows,
        "thin_for_category": thin,
        "coverage_summary": coverage_summary,
    })
    .to_string()
}
