//! Craft tab section groupings (UI + pipeline audit lists).
//!
//! Single source: `crates/core/data/craft-report-groups.json`

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::OnceLock;

const SEED_JSON: &str = include_str!("../data/craft-report-groups.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftReportGroup {
    pub id: String,
    pub label: String,
    pub subtitle: String,
    #[serde(rename = "reportIds")]
    pub report_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CraftReportGroupsFile {
    #[serde(default)]
    dedicated_handlers: Vec<String>,
    groups: Vec<CraftReportGroup>,
}

fn catalog() -> &'static CraftReportGroupsFile {
    static CAT: OnceLock<CraftReportGroupsFile> = OnceLock::new();
    CAT.get_or_init(|| {
        serde_json::from_str(SEED_JSON).expect("craft-report-groups.json must be valid JSON")
    })
}

pub fn list_craft_report_groups() -> Vec<CraftReportGroup> {
    catalog().groups.clone()
}

pub fn series_report_ids() -> Vec<String> {
    catalog()
        .groups
        .iter()
        .find(|g| g.id == "series")
        .map(|g| g.report_ids.clone())
        .unwrap_or_default()
}

pub fn manuscript_craft_audit_ids() -> Vec<String> {
    let dedicated: HashSet<&str> = catalog()
        .dedicated_handlers
        .iter()
        .map(|s| s.as_str())
        .collect();
    catalog()
        .groups
        .iter()
        .filter(|g| g.id != "series")
        .flat_map(|g| g.report_ids.iter())
        .filter(|id| !dedicated.contains(id.as_str()))
        .cloned()
        .collect()
}

pub async fn list_craft_report_groups_cmd() -> Result<Vec<CraftReportGroup>, String> {
    Ok(list_craft_report_groups())
}
