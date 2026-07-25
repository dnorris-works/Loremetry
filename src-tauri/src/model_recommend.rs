// model_recommend.rs — Auto-pick LLM models per report from task profiles + quality_bias.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::db::{self, Db};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CatalogModel {
    pub id: String,
    #[serde(default)]
    pub owned_by: Option<String>,
    /// Price as provided by the client (TokenMix is typically $/M; Claude seed is legacy).
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecommendRequest {
    /// 0.0 = prefer cheaper, 1.0 = prefer higher quality
    pub quality_bias: f64,
    pub models: Vec<CatalogModel>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportModelRec {
    pub report_id: String,
    pub model_id: String,
    pub reason: String,
    pub band: String,
    pub input_price_per_m: f64,
    pub output_price_per_m: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecommendResult {
    pub success: bool,
    pub recommendations: Vec<ReportModelRec>,
    pub error: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileSeed {
    pub report_id: String,
    pub family: String,
    pub literary: i64,
    pub reasoning: i64,
    pub creative: i64,
    pub volume: String,
    pub min_band: String,
}

#[derive(Debug, Clone)]
pub struct ModelTaskProfile {
    pub report_id: String,
    pub family: String,
    pub literary: i32,
    pub reasoning: i32,
    pub creative: i32,
    pub volume: String,
    pub min_band: String,
}

/// Normalize catalog prices to $/million tokens.
/// Heuristic: values > 50 are almost certainly already per-M (or mis-scaled);
/// values in 0.5..50 with Claude-style seeds may be per-M already (Haiku=1, Sonnet=3, Opus=15).
/// Tiny values (< 0.001) are treated as $/token → ×1e6.
pub fn price_per_million(raw: Option<f64>) -> f64 {
    let p = raw.unwrap_or(0.0);
    if p <= 0.0 {
        return 0.0;
    }
    if p < 0.001 {
        return p * 1_000_000.0;
    }
    // Already $/M (TokenMix and Anthropic list prices)
    p
}

fn band_rank(band: &str) -> i32 {
    match band {
        "budget" => 0,
        "mid" => 1,
        "literary_mid" => 2,
        "premium" => 3,
        _ => 0,
    }
}

fn model_band(input_per_m: f64) -> &'static str {
    if input_per_m <= 0.0 {
        return "budget";
    }
    if input_per_m < 0.8 {
        "budget"
    } else if input_per_m < 2.0 {
        "mid"
    } else if input_per_m < 5.0 {
        "literary_mid"
    } else {
        "premium"
    }
}

fn model_capability(id: &str, input_per_m: f64) -> (f64, f64, f64) {
    // Returns (literary, reasoning, creative) 0..5 heuristic from id + price band
    let lower = id.to_lowercase();
    let band = model_band(input_per_m);
    let base = match band {
        "budget" => (2.0, 2.5, 1.5),
        "mid" => (3.0, 3.5, 2.5),
        "literary_mid" => (4.5, 4.0, 3.5),
        _ => (5.0, 5.0, 4.5),
    };
    let mut lit: f64 = base.0;
    let mut rea: f64 = base.1;
    let mut cre: f64 = base.2;

    if lower.contains("opus") || lower.contains("fable") {
        lit = 5.0;
        rea = 5.0;
        cre = 5.0;
    } else if lower.contains("sonnet") {
        lit = 5.0;
        rea = 4.5;
        cre = 4.0;
    } else if lower.contains("haiku") || lower.contains("flash") || lower.contains("mini") || lower.contains("nano") {
        lit = lit.min(3.0);
        cre = cre.min(2.5);
    }
    if lower.contains("deepseek") && (lower.contains("flash") || lower.contains("v4-flash")) {
        lit = 2.0;
        rea = 3.0;
        cre = 1.5;
    }
    if lower.contains("o3") || lower.contains("reasoner") {
        rea = 5.0;
        cre = cre.min(3.0);
    }
    (lit, rea, cre)
}

fn cost_score(input_per_m: f64, volume: &str) -> f64 {
    // Higher = cheaper. Per-chapter volume punishes expensive models harder.
    let effective = if volume == "per_chapter" {
        input_per_m * 3.0
    } else {
        input_per_m
    };
    if effective <= 0.0 {
        return 5.0;
    }
    // Map ~0.1 → ~5, ~3 → ~2.5, ~15 → ~0.5
    let s = 5.0 - (effective.ln_1p() * 1.2);
    s.clamp(0.0, 5.0)
}

fn perf_score(profile: &ModelTaskProfile, lit: f64, rea: f64, cre: f64) -> f64 {
    let need_l = profile.literary as f64;
    let need_r = profile.reasoning as f64;
    let need_c = profile.creative as f64;
    let wsum = need_l + need_r + need_c;
    if wsum <= 0.0 {
        return 3.0;
    }
    // Soft penalty if below need
    let score_dim = |have: f64, need: f64| {
        if have >= need {
            have
        } else {
            (have / need.max(1.0)) * need * 0.6
        }
    };
    (score_dim(lit, need_l) * need_l + score_dim(rea, need_r) * need_r + score_dim(cre, need_c) * need_c) / wsum
}

fn recommend_for_profile(profile: &ModelTaskProfile, models: &[CatalogModel], bias: f64) -> Option<ReportModelRec> {
    let bias = bias.clamp(0.0, 1.0);
    let min_r = band_rank(&profile.min_band);
    let mut best: Option<(f64, ReportModelRec)> = None;

    for m in models {
        if m.id.trim().is_empty() {
            continue;
        }
        // Skip obvious non-chat
        let lower = m.id.to_lowercase();
        if lower.contains("embed")
            || lower.contains("image")
            || lower.contains("video")
            || lower.contains("tts")
            || lower.contains("whisper")
            || lower.contains("flux")
            || lower.contains("imagen")
            || lower.contains("seedance")
            || lower.contains("wan ")
        {
            continue;
        }

        let in_m = price_per_million(m.input_price);
        let out_m = price_per_million(m.output_price);
        let band = model_band(in_m);
        if band_rank(band) < min_r {
            continue;
        }
        // Economy bias: still respect floor; don't jump to premium unless bias high
        if bias < 0.75 && band == "premium" && profile.min_band != "premium" {
            // Allow premium only when quality bias is high
            continue;
        }

        let (lit, rea, cre) = model_capability(&m.id, in_m);
        // Hard floor: literary jobs need literary capability
        if profile.literary >= 4 && lit < 3.5 {
            continue;
        }
        if profile.reasoning >= 5 && rea < 3.5 {
            continue;
        }

        let perf = perf_score(profile, lit, rea, cre);
        let cost = cost_score(in_m, &profile.volume);
        let balance = (1.0 - bias) * cost + bias * perf;

        let reason = format!(
            "bias={:.2} band={} perf={:.1} cost={:.1} ({})",
            bias, band, perf, cost, profile.family
        );
        let rec = ReportModelRec {
            report_id: profile.report_id.clone(),
            model_id: m.id.clone(),
            reason,
            band: band.to_string(),
            input_price_per_m: in_m,
            output_price_per_m: out_m,
        };
        match &best {
            None => best = Some((balance, rec)),
            Some((b, _)) if balance > *b => best = Some((balance, rec)),
            _ => {}
        }
    }
    best.map(|(_, r)| r)
}

fn load_profiles(db: &Db) -> Vec<ModelTaskProfile> {
    let conn = db.0.lock().unwrap();
    db::load_model_task_profiles(&conn)
}

#[tauri::command]
pub async fn recommend_report_models(
    db: tauri::State<'_, Db>,
    request: RecommendRequest,
) -> Result<RecommendResult, String> {
    let profiles = load_profiles(&db);
    if profiles.is_empty() {
        return Ok(RecommendResult {
            success: false,
            recommendations: vec![],
            error: "No model task profiles seeded.".into(),
        });
    }
    if request.models.is_empty() {
        return Ok(RecommendResult {
            success: false,
            recommendations: vec![],
            error: "No models in catalog — fetch models in Settings first.".into(),
        });
    }

    let mut recs = Vec::new();
    for p in &profiles {
        if let Some(r) = recommend_for_profile(p, &request.models, request.quality_bias) {
            recs.push(r);
        }
    }

    // Persist autos
    {
        let conn = db.0.lock().unwrap();
        let _ = db::replace_report_model_autos(
            &conn,
            &recs
                .iter()
                .map(|r| (r.report_id.as_str(), r.model_id.as_str(), r.reason.as_str()))
                .collect::<Vec<_>>(),
        );
    }

    Ok(RecommendResult {
        success: true,
        recommendations: recs,
        error: String::new(),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveModel {
    pub report_id: String,
    pub model_id: String,
    pub source: String, // "override" | "auto" | "fallback"
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EffectiveModelsRequest {
    pub fallback_model: String,
    #[serde(default)]
    pub report_ids: Vec<String>,
}

#[tauri::command]
pub async fn get_effective_models(
    db: tauri::State<'_, Db>,
    request: EffectiveModelsRequest,
) -> Result<Vec<EffectiveModel>, String> {
    let conn = db.0.lock().unwrap();
    let overrides = db::load_report_model_overrides(&conn);
    let autos = db::load_report_model_autos(&conn);

    let ids: Vec<String> = if request.report_ids.is_empty() {
        let mut set: Vec<String> = overrides.keys().cloned().collect();
        for k in autos.keys() {
            if !set.iter().any(|x| x == k) {
                set.push(k.clone());
            }
        }
        // Also include all profiled reports
        for p in db::load_model_task_profiles(&conn) {
            if !set.iter().any(|x| x == &p.report_id) {
                set.push(p.report_id);
            }
        }
        set
    } else {
        request.report_ids
    };

    let mut out = Vec::new();
    for id in ids {
        if let Some(m) = overrides.get(&id) {
            if !m.is_empty() {
                out.push(EffectiveModel {
                    report_id: id,
                    model_id: m.clone(),
                    source: "override".into(),
                    reason: "User override".into(),
                });
                continue;
            }
        }
        if let Some((m, reason)) = autos.get(&id) {
            if !m.is_empty() {
                out.push(EffectiveModel {
                    report_id: id,
                    model_id: m.clone(),
                    source: "auto".into(),
                    reason: reason.clone(),
                });
                continue;
            }
        }
        out.push(EffectiveModel {
            report_id: id,
            model_id: request.fallback_model.clone(),
            source: "fallback".into(),
            reason: "No auto pick — using default model".into(),
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetOverrideRequest {
    pub report_id: String,
    pub model_id: String,
}

#[tauri::command]
pub async fn set_report_model_override(
    db: tauri::State<'_, Db>,
    request: SetOverrideRequest,
) -> Result<(), String> {
    let conn = db.0.lock().unwrap();
    db::set_report_model_override(&conn, &request.report_id, &request.model_id)
}

#[tauri::command]
pub async fn clear_report_model_override(
    db: tauri::State<'_, Db>,
    report_id: String,
) -> Result<(), String> {
    let conn = db.0.lock().unwrap();
    db::clear_report_model_override(&conn, &report_id)
}

#[tauri::command]
pub async fn list_report_model_overrides(
    db: tauri::State<'_, Db>,
) -> Result<HashMap<String, String>, String> {
    let conn = db.0.lock().unwrap();
    Ok(db::load_report_model_overrides(&conn))
}

// Re-export helpers for db seeding
pub fn profile_from_seed(s: ProfileSeed) -> ModelTaskProfile {
    ModelTaskProfile {
        report_id: s.report_id,
        family: s.family,
        literary: s.literary as i32,
        reasoning: s.reasoning as i32,
        creative: s.creative as i32,
        volume: s.volume,
        min_band: s.min_band,
    }
}

pub fn parse_profile_seeds(json: &str) -> Result<Vec<ModelTaskProfile>, String> {
    let seeds: Vec<ProfileSeed> =
        serde_json::from_str(json).map_err(|e| format!("model-task-profiles.json: {}", e))?;
    Ok(seeds.into_iter().map(profile_from_seed).collect())
}
