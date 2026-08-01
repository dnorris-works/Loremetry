// winningcat.rs — one-time importer for the WinningCat Amazon browse-node
// dataset (https://winningcat.com). Populates kdp_categories with real,
// ID-backed category paths for the "Books" and "Kindle Store" departments
// only — the rest of the 34,000+ node dataset (Automotive, Electronics,
// Home & Kitchen, etc.) is irrelevant to this app and is skipped on import.
//
// Expected row format (ragged — depth varies by row), one full path per row:
//   Books (1000),Arts & Photography (1),Architecture (173508),Buildings (266162)
//   Kindle Store (133141011),Literature & Fiction (157296011),...
//
// Each cell is "Name (NodeID)". The department (first cell) determines the
// store ("Books" -> print, "Kindle Store" -> ebook); it's then dropped from
// the stored path since store already implies it, matching the convention
// used everywhere else in kdp_categories (no "Kindle Store >" prefix).

use serde::Serialize;

use crate::app_ctx::AppCtx;
use crate::db;

#[derive(Serialize)]
pub struct ImportResult {
    pub success:                   bool,
    pub imported:                  usize,
    pub skipped_other_department:  usize,
    pub skipped_unparseable:       usize,
    pub stale_count:                usize,   // in the catalog from a previous import, missing from this one
    pub imported_at:                String,  // pass to remove_stale_kdp_categories to clean these up
    pub error:                     String,
}

fn fail(msg: &str) -> ImportResult {
    ImportResult { success: false, imported: 0, skipped_other_department: 0, skipped_unparseable: 0, stale_count: 0, imported_at: String::new(), error: msg.to_string() }
}

pub async fn import_winningcat_csv(app: AppCtx, csv_text: String) -> ImportResult {
    if csv_text.trim().is_empty() {
        return fail("CSV text is empty.");
    }
    let content = csv_text;

    // Captured BEFORE this import touches anything — any WinningCat-sourced
    // row whose last_seen_at is still older than this once we're done was
    // in a previous file but isn't in this one. That's real drift (Amazon
    // retired/renamed the category), not something to silently ignore.
    let import_started_at = chrono::Utc::now().to_rfc3339();

    let pool = &app.db.pool;

    let mut imported = 0usize;
    let mut skipped_dept = 0usize;
    let mut skipped_bad = 0usize;

    for line in content.lines() {
        if line.trim().is_empty() { continue; }

        let cells = parse_csv_line(line);
        if cells.is_empty() { continue; }

        let mut parsed: Vec<(String, String)> = Vec::new(); // (name, node_id)
        let mut ok = true;
        for cell in &cells {
            if cell.trim().is_empty() {
                continue;
            }
            match parse_node_cell(cell) {
                Some(pair) => parsed.push(pair),
                None => { ok = false; break; }
            }
        }
        if !ok || parsed.is_empty() {
            skipped_bad += 1;
            continue;
        }

        let dept = parsed[0].0.to_lowercase();
        let store = if dept == "kindle store" { "Kindle" }
                    else if dept == "books"        { "Books" }
                    else { skipped_dept += 1; continue; };

        let rest = &parsed[1..];
        if rest.is_empty() { skipped_bad += 1; continue; }

        let path    = rest.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(" > ");
        let node_id = &rest.last().unwrap().1;

        match db::import_kdp_category(pool, &path, store, node_id).await {
            Ok(())  => imported += 1,
            Err(_)  => skipped_bad += 1,
        }
    }

    let stale = db::stale_winningcat_paths(pool, &import_started_at).await;

    ImportResult {
        success: true,
        imported,
        skipped_other_department: skipped_dept,
        skipped_unparseable:      skipped_bad,
        stale_count: stale.len(),
        imported_at: import_started_at,
        error: String::new(),
    }
}

#[derive(Serialize)]
pub struct StaleCleanupResult {
    pub success: bool,
    pub removed: usize,
    pub error:   String,
}

#[derive(Serialize)]
pub struct CatalogStatusResult {
    pub success:        bool,
    pub has_data:       bool,
    pub ready:          bool,
    pub kindle_count:   i64,
    pub books_count:    i64,
    pub total_count:    i64,
    pub last_import_at: String,
    pub message:        String,
    pub error:          String,
}

fn format_catalog_message(status: &db::WinningCatCatalogStatus) -> String {
    if status.ready {
        let when = status
            .last_import_at
            .as_deref()
            .map(|t| format!(" Last import: {t}."))
            .unwrap_or_default();
        return format!(
            "WinningCat catalog is in the database — {} Kindle and {} Books categories.{when}",
            status.kindle_count, status.books_count
        );
    }
    if status.has_data {
        return format!(
            "Partial WinningCat catalog — {} Kindle and {} Books categories. \
             Import a full CSV for reliable category matching (need at least 50 per store).",
            status.kindle_count, status.books_count
        );
    }
    "No WinningCat data in the database. Import the CSV to enable KDP category matching.".to_string()
}

pub async fn get_winningcat_catalog_status(app: AppCtx) -> CatalogStatusResult {
    let status = db::winningcat_catalog_status(&app.db.pool).await;
    CatalogStatusResult {
        success: true,
        has_data: status.has_data,
        ready: status.ready,
        kindle_count: status.kindle_count,
        books_count: status.books_count,
        total_count: status.total_count,
        last_import_at: status.last_import_at.clone().unwrap_or_default(),
        message: format_catalog_message(&status),
        error: String::new(),
    }
}

/// Delete every WinningCat-sourced category not seen in the import that
/// started at `since`. Only ever called explicitly by the user after
/// reviewing the stale count from an import — never automatic, since a
/// category disappearing from one file could be a CSV quirk, not a real
/// Amazon retirement.
pub async fn remove_stale_kdp_categories(app: AppCtx, since: String) -> StaleCleanupResult {
    match db::remove_stale_winningcat_paths(&app.db.pool, &since).await {
        Ok(removed) => StaleCleanupResult { success: true, removed, error: String::new() },
        Err(e) => StaleCleanupResult { success: false, removed: 0, error: e },
    }
}

/// Parse a single "Name (NodeID)" cell. Returns None for anything that
/// doesn't match — a header row, a malformed export, etc.
fn parse_node_cell(cell: &str) -> Option<(String, String)> {
    let cell = cell.trim();
    let open  = cell.rfind('(')?;
    let close = cell.rfind(')')?;
    if close < open { return None; }
    let id = &cell[open + 1..close];
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) { return None; }
    let name = cell[..open].trim().to_string();
    if name.is_empty() { return None; }
    Some((name, id.to_string()))
}

/// Quote-aware CSV line splitter (category names can contain commas).
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                if in_quotes && chars.peek() == Some(&'"') { chars.next(); current.push('"'); }
                else { in_quotes = !in_quotes; }
            }
            ',' if !in_quotes => { fields.push(current.trim().to_string()); current = String::new(); }
            _ => current.push(ch),
        }
    }
    fields.push(current.trim().to_string());
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_trailing_empty_csv_fields() {
        let line = "Books (1000),Arts & Photography (1),Criticism (1002),";
        let cells = parse_csv_line(line);
        let mut parsed = Vec::new();
        for cell in &cells {
            if cell.trim().is_empty() {
                continue;
            }
            parsed.push(parse_node_cell(cell).expect("cell"));
        }
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[2], ("Criticism".into(), "1002".into()));
    }
}
