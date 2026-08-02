// analysis/pipeline.rs — Orchestration commands that compose the analysis pipeline.
//
// These commands call into chapters, genres, categories, keywords, and bisac
// to run multi-step analyses and assemble combined reports.

use super::{emit, err, GenreResult, FolderRequest, AnalyzeStoryRequest};
use crate::app_ctx::AppCtx;
use crate::db;
use crate::documents;
use crate::models::KeywordResult;

use super::chapters::{any_chapter_needs_summary, phase1_config_from, phase1_summaries};
use super::genres::{RankedGenre, ai_rank_genres, phase2_analyze, render_full_report};
use super::categories::{match_categories_by_store, rank_by_discoverability};
use super::bisac::ai_pick_bisac;
use super::content_advisory::{aggregate_content_signals, generate_content_maturity_advisory};
use super::keywords::{
    call_keyword_optimizer, call_keyword_optimizer_with_pool,
    derive_keyword_seeds, derive_wide_keyword_seeds,
    run_keyword_searches_canopy, run_keyword_searches_dataforseo,
    run_google_keyword_searches_dataforseo,
    generate_discovery_keywords, generate_mi_search_terms, render_kdp_keywords, render_search_terms,
};

fn has_dataforseo_creds(login: &str, password: &str) -> bool {
    !login.trim().is_empty() && !password.trim().is_empty()
}

#[derive(Clone, Copy)]
struct PublishFormats {
    ebook: bool,
    print: bool,
}

impl PublishFormats {
    fn from_flags(ebook: bool, print: bool) -> Self {
        if !ebook && !print {
            Self { ebook: true, print: true }
        } else {
            Self { ebook, print }
        }
    }
}

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct ReportFreshness {
    pub doc_type: String,
    pub status:   String,
}

#[derive(serde::Serialize)]
pub struct AnalysisState {
    pub has_folder:                 bool,
    pub summary_count:              usize,
    pub summary_chapter_count:      usize,
    pub summary_missing_count:      usize,
    pub summary_stale_count:        usize,
    pub summary_missing_files:      Vec<String>,
    pub summary_stale_files:        Vec<String>,
    pub has_genre_data:             bool,
    pub has_full_report:            bool,
    pub has_wide_analysis:          bool,
    pub has_keywords:               bool,
    pub has_search_terms:           bool,
    pub has_competition:            bool,
    pub has_categories:             bool,
    pub has_genre_ranking:          bool,
    pub has_mapped_verified:        bool,
    pub has_bisac:                  bool,
    pub has_discovery_keywords:     bool,
    pub has_keyword_search_results: bool,
    pub has_google_keyword_search:  bool,
    pub has_zeigarnik:              bool,
    pub has_readability:            bool,
    pub has_continuity_check:       bool,
    pub has_show_dont_tell:         bool,
    pub has_ai_isms:                bool,
    pub existing_docs:              Vec<String>,
    pub report_freshness:           Vec<ReportFreshness>,
    pub manuscript_fingerprint:     String,
}

// ── Analysis state check ──────────────────────────────────────────────────────

async fn doc_is_fresh(pool: &sqlx::PgPool, story_id: &str, doc_type: &str, fp: &str) -> bool {
    db::report_freshness_status(pool, story_id, doc_type, fp).await == db::Freshness::Fresh
}

pub async fn check_analysis_state(app: AppCtx, story_id: String) -> AnalysisState {
    let database = app.db.as_ref();
    let pool = &database.pool;
    let has_folder = crate::stories::story_exists(database, &story_id).await;

    let chapters = documents::list_chapters_db(&app.db, &story_id)
        .await
        .unwrap_or_default();
    let current_fp =
        crate::manuscript_fingerprint::compute_manuscript_fingerprint(&chapters);
    let summary_hashes = db::load_chapter_summary_hashes(pool, &story_id).await;

    let mut summary_missing_count = 0usize;
    let mut summary_stale_count = 0usize;
    let mut summary_missing_files: Vec<String> = Vec::new();
    let mut summary_stale_files: Vec<String> = Vec::new();

    for chapter in &chapters {
        let file = documents::chapter_display_name(chapter);
        let cleaned = crate::manuscript_fingerprint::clean_for_ai(&chapter.content);
        if cleaned.is_empty() {
            continue;
        }
        let current_hash = crate::manuscript_fingerprint::chapter_source_hash(&cleaned);

        match summary_hashes.get(&file) {
            None => {
                summary_missing_count += 1;
                summary_missing_files.push(file);
            }
            Some(stored_hash) if stored_hash.is_empty() || stored_hash != &current_hash => {
                summary_stale_count += 1;
                summary_stale_files.push(file);
            }
            Some(_) => {
                // Hash matches — still stale if the stored row is not AI prose.
                if !db::chapter_has_current_summary(pool, &story_id, &file, &current_hash).await {
                    summary_stale_count += 1;
                    summary_stale_files.push(file);
                }
            }
        }
    }

    let report_doc_types = [
        "genre_analysis",
        "genre_ranking",
        "genres_and_categories",
        "kdp_categories",
        "kdp_keywords",
        "bisac_classification",
        "mi_search_terms",
        "discovery_keywords",
        "google_keyword_search",
        "wide_metadata_paste",
        "wide_analysis",
        "analysis",
        "full_report",
        "keyword_search",
        "competition_report",
        "review_mining",
        "author_analysis",
        "zeigarnik_analysis",
        "readability_analysis",
        "continuity_check",
        "show_dont_tell",
        "ai_isms",
        "content_maturity_advisory",
        "hook_strength",
        "line_polish",
        "blurb_builder",
        "vellum_prep",
        "print_production",
    ];
    let report_freshness: Vec<ReportFreshness> = {
        let mut rows = Vec::new();
        for dt in report_doc_types {
            rows.push(ReportFreshness {
                doc_type: dt.to_string(),
                status: db::report_freshness_status(pool, &story_id, dt, &current_fp)
                    .await
                    .as_str()
                    .to_string(),
            });
        }
        rows
    };

    let existing_docs = db::list_existing_doc_types(pool, &story_id).await;

    let genre_fresh = doc_is_fresh(pool, &story_id, "genres_and_categories", &current_fp).await;

    AnalysisState {
        has_folder,
        summary_count:              db::chapter_summary_count(pool, &story_id).await as usize,
        summary_chapter_count:      chapters.len(),
        summary_missing_count,
        summary_stale_count,
        summary_missing_files,
        summary_stale_files,
        has_genre_data:             db::load_genre_data(pool, &story_id).await.is_some() && genre_fresh,
        has_full_report:            doc_is_fresh(pool, &story_id, "analysis", &current_fp).await,
        has_wide_analysis:          doc_is_fresh(pool, &story_id, "wide_analysis", &current_fp).await,
        has_keywords:               db::load_kdp_keywords(pool, &story_id).await.is_some()
            && doc_is_fresh(pool, &story_id, "kdp_keywords", &current_fp).await,
        has_search_terms:           !db::load_mi_search_terms(pool, &story_id).await.is_empty()
            && doc_is_fresh(pool, &story_id, "mi_search_terms", &current_fp).await,
        has_competition:            doc_is_fresh(pool, &story_id, "competition_report", &current_fp).await,
        has_categories:             db::has_category_results(pool, &story_id).await
            && doc_is_fresh(pool, &story_id, "genres_and_categories", &current_fp).await,
        has_genre_ranking:          db::has_genre_rankings(pool, &story_id).await && genre_fresh,
        has_mapped_verified:        doc_is_fresh(pool, &story_id, "mapped_categories", &current_fp).await,
        has_bisac:                  db::has_bisac_classifications(pool, &story_id).await
            && doc_is_fresh(pool, &story_id, "bisac_classification", &current_fp).await,
        has_discovery_keywords:     !db::load_discovery_keywords(pool, &story_id).await.is_empty()
            && doc_is_fresh(pool, &story_id, "discovery_keywords", &current_fp).await,
        has_keyword_search_results: db::has_keyword_search_results(pool, &story_id).await
            && doc_is_fresh(pool, &story_id, "keyword_search", &current_fp).await,
        has_google_keyword_search:  db::has_google_keyword_search_results(pool, &story_id).await
            && doc_is_fresh(pool, &story_id, "google_keyword_search", &current_fp).await,
        has_zeigarnik:              db::has_zeigarnik_analysis(pool, &story_id).await
            && doc_is_fresh(pool, &story_id, "zeigarnik_analysis", &current_fp).await,
        has_readability:            db::get_document(pool, &story_id, "readability_analysis").await.is_some()
            && doc_is_fresh(pool, &story_id, "readability_analysis", &current_fp).await,
        has_continuity_check:       doc_is_fresh(pool, &story_id, "continuity_check", &current_fp).await,
        has_show_dont_tell:         doc_is_fresh(pool, &story_id, "show_dont_tell", &current_fp).await,
        has_ai_isms:                doc_is_fresh(pool, &story_id, "ai_isms", &current_fp).await,
        existing_docs,
        report_freshness,
        manuscript_fingerprint:     current_fp,
    }
}

// ── run_everything ────────────────────────────────────────────────────────────

/// Run everything except folder selection and chapter summaries:
/// Analyze Genre → Full Analysis → Optimize Keywords → Generate Search Terms
pub async fn run_everything(app: AppCtx, request: FolderRequest) -> GenreResult {
    if !crate::stories::story_exists(&app.db, &request.story_id).await {
        return err("Story not found.");
    }

    crate::reset_cancel();
    let database = app.db.as_ref();
    let run_ts = chrono::Utc::now().to_rfc3339();

    // ── Step 1: Ensure summaries exist ────────────────────────────────────
    let mut summaries = db::load_chapter_summaries(&database.pool, &request.story_id).await;
    if summaries.is_empty() {
        emit(&app, "Step 1: No summaries found — generating now...");
        let chapters = documents::list_chapters_db(&app.db, &request.story_id).await.unwrap_or_default();
        if chapters.is_empty() { return err("No chapter documents found. Upload manuscript chapters first."); }
        let config = phase1_config_from(&request.provider, &request.api_key, &request.model, &request.summaries_model, false);
        phase1_summaries(&app, &database, &chapters, &request.story_id, &config).await;
        summaries = db::load_chapter_summaries(&database.pool, &request.story_id).await;
                if summaries.is_empty() { return err("Could not produce chapter summaries."); }
            } else {
                emit(&app, &format!("Step 1: {} summaries found — skipping.", summaries.len()));
            }
            if crate::is_cancelled() { return err("Cancelled."); }
    // ── Step 2: Genre analysis ─────────────────────────────────────────────
    emit(&app, "Step 2: Running genre analysis...");
    let genre_result = phase2_analyze(&app, &database, &request.story_id, &summaries, &request.provider, &request.api_key, &request.model, &request.genre_model).await;
    if !genre_result.success { return genre_result; }
    if crate::is_cancelled() { return err("Cancelled."); }

    // ── Step 3: Full report ────────────────────────────────────────────────
    emit(&app, "Step 3: Building full report...");
    let genre_data = db::load_genre_data(&database.pool, &request.story_id).await;
    let genre_data = match genre_data {
        Some(d) => d,
        None    => return err("genre_data missing after analysis."),
    };
    let full_report = render_full_report(&genre_data, false);
    let _ = db::save_document_at(&database.pool, &request.story_id, "full_report", &full_report, &run_ts).await;
    emit(&app, "  ✓ Full report saved to database.");
    if crate::is_cancelled() { return err("Cancelled."); }

    // ── Step 4: Optimize KDP keywords ─────────────────────────────────────
    emit(&app, "Step 4: Optimizing KDP keywords...");
    match call_keyword_optimizer(&app, &request.story_id, &request.provider, &request.api_key, &request.model, &genre_data, &genre_data.genre_signals).await {
        Ok((entries, strategy)) => {
            let _ = db::save_kdp_keywords(&database.pool, &request.story_id, &entries, &strategy, "*(Generated from genre analysis.)*").await;
                    let rendered = render_kdp_keywords(&entries, &strategy, "*(Generated from genre analysis.)*");
                    let _ = db::save_document_at(&database.pool, &request.story_id, "kdp_keywords", &rendered, &run_ts).await;
                    emit(&app, "  ✓ KDP keywords saved to database.");
                }
                Err(e) => emit(&app, &format!("  ⚠ Keyword optimization failed: {}", e)),
            }
            if crate::is_cancelled() { return err("Cancelled."); }
    // ── Step 5: Generate search terms ──────────────────────────────────────
    emit(&app, "Step 5: Generating competition search terms...");
    match generate_mi_search_terms(&app, &request.story_id, &request.provider, &request.api_key, &request.model, &genre_data).await {
        Ok(keywords) => {
            let _ = db::save_mi_search_terms(&database.pool, &request.story_id, &keywords).await;
                    let rendered = render_search_terms(&keywords);
                    let _ = db::save_document_at(&database.pool, &request.story_id, "mi_search_terms", &rendered, &run_ts).await;
                    emit(&app, &format!("  ✓ {} search terms saved to database.", keywords.len()));
                    for kw in &keywords { emit(&app, &format!("    • {}", kw)); }
                }
                Err(e) => emit(&app, &format!("  ⚠ Search terms generation failed: {}", e)),
            }
    emit(&app, "✓ Analysis complete. Run Analyze Competition next.");

    GenreResult { success: true, report: full_report, error: String::new(), run_ts: run_ts.clone() }
}

// ── run_full_analysis ─────────────────────────────────────────────────────────

pub async fn run_full_analysis(app: AppCtx, request: FolderRequest) -> GenreResult {
    if !crate::stories::story_exists(&app.db, &request.story_id).await {
        return err("Story not found.");
    }

    let database = app.db.as_ref();
    let run_ts = chrono::Utc::now().to_rfc3339();

    // ── Phase 1 ──────────────────────────────────────────────────────────
    let mut summaries = db::load_chapter_summaries(&database.pool, &request.story_id).await;
    if summaries.is_empty() {
        emit(&app, "Phase 1: Generating chapter summaries...");
        let chapters = documents::list_chapters_db(&app.db, &request.story_id).await.unwrap_or_default();
        if chapters.is_empty() { return err("No chapter documents found. Upload manuscript chapters first."); }
        let config = phase1_config_from(&request.provider, &request.api_key, &request.model, &request.summaries_model, false);
        phase1_summaries(&app, &database, &chapters, &request.story_id, &config).await;
        summaries = db::load_chapter_summaries(&database.pool, &request.story_id).await;
            } else {
                emit(&app, &format!("Phase 1: {} summaries already exist — skipping.", summaries.len()));
            }
            if summaries.is_empty() { return err("No chapter summaries available."); }
    // ── Phase 2 ──────────────────────────────────────────────────────────
    let existing = db::load_genre_data(&database.pool, &request.story_id).await;
    let genre_data = if let Some(d) = existing {
        emit(&app, "Phase 2: genre data exists in database — loading...");
        d
    } else {
        emit(&app, "Phase 2: Running genre analysis...");
        let r = phase2_analyze(&app, &database, &request.story_id, &summaries, &request.provider, &request.api_key, &request.model, &request.genre_model).await;
        if !r.success { return r; }
        match db::load_genre_data(&database.pool, &request.story_id).await {
            Some(d) => d,
            None    => return err("Phase 2 produced no genre data."),
        }
    };
    emit(&app, &format!("  KDP ebook paths: {}", genre_data.kdp_ebook.join(", ")));
    emit(&app, &format!("  KDP print paths: {}", genre_data.kdp_print.join(", ")));

    // ── Build full report ─────────────────────────────────────────────────
    emit(&app, "Building full report...");
    let full_report = render_full_report(&genre_data, true);
    let _ = db::save_document_at(&database.pool, &request.story_id, "full_report", &full_report, &run_ts).await;
    emit(&app, "✓ Full report saved to database.");

    GenreResult { success: true, report: full_report, error: String::new(), run_ts: run_ts.clone() }
}

// ── find_genres_and_categories_for_story ──────────────────────────────────────

pub async fn find_genres_and_categories_for_story(app: AppCtx, request: FolderRequest) -> GenreResult {
    let cancel = crate::cancel_notify();
    tokio::select! {
        result = find_genres_and_categories_inner(app, request) => result,
        _ = cancel.notified() => err("Cancelled."),
    }
}

async fn find_genres_and_categories_inner(app: AppCtx, request: FolderRequest) -> GenreResult {
    let database = app.db.as_ref();
    let run_ts = chrono::Utc::now().to_rfc3339();

    // ── Ensure genre_data exists ──
    let mut genre_data = db::load_genre_data(&database.pool, &request.story_id).await;
    if genre_data.is_none() {
        emit(&app, "No genre data yet — running Analyze first...");
        if !crate::stories::story_exists(&app.db, &request.story_id).await {
            return err("Story not found.");
        }

        let mut summaries = db::load_chapter_summaries(&database.pool, &request.story_id).await;
        if summaries.is_empty() {
            let chapters = documents::list_chapters_db(&app.db, &request.story_id).await.unwrap_or_default();
            if chapters.is_empty() { return err("No chapter documents found. Upload manuscript chapters first."); }
            let config = phase1_config_from(&request.provider, &request.api_key, &request.model, &request.summaries_model, false);
            phase1_summaries(&app, &database, &chapters, &request.story_id, &config).await;
            summaries = db::load_chapter_summaries(&database.pool, &request.story_id).await;
                }
                if summaries.is_empty() { return err("Could not produce chapter summaries."); }
        let r = phase2_analyze(&app, &database, &request.story_id, &summaries, &request.provider, &request.api_key, &request.model, &request.genre_model).await;
        if !r.success { return err(&r.error); }
        genre_data = db::load_genre_data(&database.pool, &request.story_id).await;
    }
    let genre_data = match genre_data {
        Some(d) => d,
        None    => return err("Could not produce genre data."),
    };

    let mut report_sections: Vec<String> = Vec::new();

    // ── Rank Genres ────────────────────────────────────────────────────
    emit(&app, "Ranking manuscript against master genre list...");
    let ranked: Vec<RankedGenre> = {
        let master_list = crate::genre_taxonomy::master_genre_list(&database).await
            .map_err(|e| format!("Could not load genre list from database: {}", e));
        let master_list = match master_list {
            Ok(l) => l,
            Err(e) => return err(&e),
        };

        let description = format!(
            "{}\n\nKDP paths already identified: {}\n\n{}",
            genre_data.industry_ebook, genre_data.kdp_ebook.join("; "), genre_data.genre_signals
        );

        let ai_ranked = match ai_rank_genres(&app, &request.story_id, &request.provider, &request.api_key, &request.model, &request.genre_model, &description, &master_list).await {
            Ok(r) => r,
            Err(e) => return err(&format!("Genre ranking failed: {}", e)),
        };

        let mut ranked: Vec<RankedGenre> = Vec::new();
        for r in ai_ranked {
            let kdp_paths = crate::genre_taxonomy::kdp_paths_for_genre(&database, &r.genre, "Kindle")
                .await
                .unwrap_or_default();
            ranked.push(RankedGenre { genre: r.genre, confidence: r.confidence, reason: r.reason, kdp_paths });
        }
        ranked.sort_by(|a, b| b.confidence.cmp(&a.confidence));

        let rows: Vec<(String, u8, String)> = ranked.iter().map(|r| (r.genre.clone(), r.confidence, r.reason.clone())).collect();
        let _ = db::replace_genre_rankings(&database.pool, &request.story_id, &rows).await;
        let genre_ranking_md = {
            let mut s = vec!["# Genre Ranking".to_string(), String::new()];
            for r in &ranked { s.push(format!("## {} — {}%", r.genre, r.confidence)); s.push(String::new()); s.push(r.reason.clone()); s.push(String::new()); }
            s.join("\n")
        };
        let _ = db::save_document_at(&database.pool, &request.story_id, "genre_ranking", &genre_ranking_md, &run_ts).await;

        ranked
    };
    for r in &ranked { emit(&app, &format!("  {}% — {}", r.confidence, r.genre)); }

    report_sections.push({
        let mut s = vec!["## Genre Ranking".to_string(), String::new(),
            "Scored independently — percentages do not sum to 100.".to_string(), String::new()];
        for r in &ranked { s.push(format!("- **{}** — {}%", r.genre, r.confidence)); }
        s.push(String::new());
        s.join("\n")
    });
    let genre_terms: Vec<(String, u8)> = if !ranked.is_empty() {
        ranked.iter().filter(|r| r.confidence >= 30).take(6).map(|r| (r.genre.clone(), r.confidence)).collect()
    } else {
        vec![(genre_data.industry_ebook.clone(), 100)]
    };

    // ── KDP Categories, both formats ──
    emit(&app, "Matching KDP categories against the imported catalog...");
    let base_description = format!("{}\n\n{}", genre_data.industry_ebook, genre_data.genre_signals);
    let mut kdp_section = vec!["## KDP Categories".to_string(), String::new()];
    for (store, label) in [("Kindle", "Kindle eBook"), ("Books", "Paperback")] {
        kdp_section.push(format!("### {}", label));
        kdp_section.push(String::new());
        let total_catalog = db::kdp_category_count(&database.pool, store).await;
        if total_catalog < 50 {
            kdp_section.push("*Catalog nearly empty for this store — import WinningCat data, or use Find Categories (PR).*".to_string());
            kdp_section.push(String::new());
            continue;
        }

        let result = match_categories_by_store(&app, &database, &request.story_id, store, &base_description, &genre_terms, &request.provider, &request.api_key, &request.model).await;

        let final_cats = rank_by_discoverability(&app, store, result.qualifying, &request.canopy_api_key).await;

        if final_cats.is_empty() {
            kdp_section.push("*No candidates cleared the fit bar for this store.*".to_string());
        } else {
            for (i, q) in final_cats.iter().enumerate() {
                let bonus = if i >= 3 { " — bonus candidate for post-launch" } else { "" };
                let disc_note = if q.verified {
                    format!(" — sales to #10: {}", q.sales_to_ten)
                } else {
                    " — could not verify live".to_string()
                };
                kdp_section.push(format!("{}. `{}` (fit {}%){}{} — matched by: {}", i + 1, q.path, q.fit_confidence, bonus, disc_note, q.agreeing_genres.join(", ")));
                if !q.top_books.is_empty() {
                    kdp_section.push(String::new());
                    kdp_section.push("   **Current Top Sellers:**".to_string());
                    for (rank, book) in q.top_books.iter().enumerate() {
                        let amazon_link = format!("https://www.amazon.com/dp/{}", book.asin);
                        let img_tag = book.image_url.as_deref()
                            .map(|url| format!("   <img src=\"{}\" height=\"60\" /> ", url))
                            .unwrap_or_default();
                        kdp_section.push(format!("   {}{}. [{}]({})", img_tag, rank + 1, book.title, amazon_link));
                    }
                    kdp_section.push(String::new());
                }
            }
        }
        kdp_section.push(String::new());
    }
    report_sections.push(kdp_section.join("\n"));

    // ── BISAC, ebook then print if different ───────────────────────
    emit(&app, "Classifying BISAC subject headings...");
    let bisac_master = db::master_bisac_list(&database.pool).await;
    let same_as_ebook = genre_data.industry_print.trim().eq_ignore_ascii_case(genre_data.industry_ebook.trim());

    let ebook_desc = format!("{}\n\n{}", genre_data.industry_ebook, genre_data.genre_signals);
    let ebook_picks = ai_pick_bisac(&app, &request.story_id, &request.provider, &request.api_key, &request.model, &ebook_desc, &bisac_master).await.unwrap_or_default();
    {
        let rows: Vec<(String, String, u8, String)> = ebook_picks.iter().map(|(c, h, cf, r)| (c.clone(), h.clone(), *cf, r.clone())).collect();
                let _ = db::replace_bisac_classifications(&database.pool, &request.story_id, "ebook", &rows).await;
            }
    let print_picks_opt = if same_as_ebook {
        let rows: Vec<(String, String, u8, String)> = ebook_picks.iter().map(|(c, h, cf, r)| (c.clone(), h.clone(), *cf, r.clone())).collect();
                let _ = db::replace_bisac_classifications(&database.pool, &request.story_id, "print", &rows).await;
                None
            } else {
                let print_desc = format!("{}\n\n{}", genre_data.industry_print, genre_data.genre_signals);
                let print_picks = ai_pick_bisac(&app, &request.story_id, &request.provider, &request.api_key, &request.model, &print_desc, &bisac_master).await.unwrap_or_default();
                let rows: Vec<(String, String, u8, String)> = print_picks.iter().map(|(c, h, cf, r)| (c.clone(), h.clone(), *cf, r.clone())).collect();
                let _ = db::replace_bisac_classifications(&database.pool, &request.story_id, "print", &rows).await;
                Some(print_picks)
            };
    let mut bisac_section = vec![
        "## BISAC Classification".to_string(), String::new(),
        "*Verify against BISG's free lookup (bisg.org/complete-bisac-subject-headings-list) before submitting anywhere. Kindle eBook no longer takes BISAC directly on KDP; this matters for KDP Print and wide/Ingram distribution. No live discoverability data exists for BISAC — close calls are broken by preferring a specific heading over a generic \"/ General\" one, a structural heuristic, not measured data.*".to_string(),
        String::new(),
    ];

    bisac_section.push("### Ebook".to_string());
    bisac_section.push(String::new());
    if ebook_picks.is_empty() {
        bisac_section.push("*No confident BISAC match.*".to_string());
    } else {
        for (i, (code, heading, conf, _reason)) in ebook_picks.iter().enumerate() {
            bisac_section.push(format!("{}. `{}` — {} ({}%)", i + 1, code, heading, conf));
        }
    }
    bisac_section.push(String::new());

    bisac_section.push("### Print".to_string());
    bisac_section.push(String::new());
    match &print_picks_opt {
        None => bisac_section.push("*Same as ebook — print genre tag matches ebook.*".to_string()),
        Some(print_picks) => {
            let ebook_codes: std::collections::HashSet<String> = ebook_picks.iter().map(|(c, _, _, _)| c.clone()).collect();
            let print_codes: std::collections::HashSet<String> = print_picks.iter().map(|(c, _, _, _)| c.clone()).collect();
            if !print_picks.is_empty() && ebook_codes == print_codes {
                bisac_section.push("*Same codes as ebook.*".to_string());
            } else if print_picks.is_empty() {
                bisac_section.push("*No confident BISAC match.*".to_string());
            } else {
                for (i, (code, heading, conf, _reason)) in print_picks.iter().enumerate() {
                    bisac_section.push(format!("{}. `{}` — {} ({}%)", i + 1, code, heading, conf));
                }
            }
        }
    }
    bisac_section.push(String::new());
    report_sections.push(bisac_section.join("\n"));

    // ── Positioning context ──
    let mut context_section = vec!["## Positioning Context".to_string(), String::new()];
    context_section.push(format!("**Reader demographic:** {}", genre_data.reader_demographic));
    context_section.push(format!("**Bookstore shelving:** {}", genre_data.bookstore_shelving));
    if !genre_data.comps_ebook.is_empty() {
        context_section.push(String::new());
        context_section.push("**Ebook comps:**".to_string());
        for c in &genre_data.comps_ebook { context_section.push(format!("- {}", c)); }
    }
    if !genre_data.comps_print.is_empty() {
        context_section.push(String::new());
        context_section.push("**Print comps:**".to_string());
        for c in &genre_data.comps_print { context_section.push(format!("- {}", c)); }
    }
    context_section.push(String::new());
    report_sections.push(context_section.join("\n"));

    let now = chrono::Utc::now().format("%B %-d, %Y %H:%M UTC").to_string();
    let mut lines = vec![
        "# Find Genres & Categories".to_string(),
        format!("Generated: {}", now),
        "Full pipeline in one pass: genre ranking, KDP categories (Kindle eBook + Paperback, verified live via Canopy API), BISAC classification (ebook + print), and positioning context.".to_string(),
        String::new(), "---".to_string(), String::new(),
    ];
    lines.push(report_sections.join("\n---\n\n"));
    let report = lines.join("\n");

    let _ = db::save_document_at(&database.pool, &request.story_id, "genres_and_categories", &report, &run_ts).await;
    emit(&app, "✓ Genres & Categories report saved to database.");

    GenreResult { success: true, report, error: String::new(), run_ts: run_ts.clone() }
}

// ── Combined Report Assembly ──────────────────────────────────────────────────

/// Assembles all pipeline output sections into a single structured JSON document.
pub(crate) fn render_combined_report(
    kdp_paste_section: &str,
    genre_ranking_section: &str,
    kdp_categories_section: &str,
    bisac_section: &str,
    kdp_keywords_section: &str,
    discovery_keywords_section: &str,
    positioning_section: &str,
    description_snippet: Option<&str>,
) -> String {
    let json = serde_json::json!({
        "schema": "analysis_v1",
        "sections": {
            "kdp_paste": kdp_paste_section,
            "genre_ranking": genre_ranking_section,
            "kdp_categories": kdp_categories_section,
            "bisac": bisac_section,
            "kdp_keywords": kdp_keywords_section,
            "discovery_keywords": discovery_keywords_section,
            "positioning": positioning_section,
            "description": description_snippet.unwrap_or(""),
        }
    });
    json.to_string()
}

/// Assembles wide-distribution outputs into one saved report.
pub(crate) fn render_wide_combined_report(
    genre_ranking_section: &str,
    bisac_section: &str,
    discovery_keywords_section: &str,
    google_keywords_section: &str,
    content_advisory_section: &str,
    wide_paste_section: &str,
    positioning_section: &str,
) -> String {
    serde_json::json!({
        "schema": "wide_analysis_v1",
        "sections": {
            "genre_ranking": genre_ranking_section,
            "bisac": bisac_section,
            "discovery_keywords": discovery_keywords_section,
            "google_keywords": google_keywords_section,
            "content_advisory": content_advisory_section,
            "wide_paste": wide_paste_section,
            "positioning": positioning_section,
        }
    }).to_string()
}

// ── analyze_story selection gates ─────────────────────────────────────────────

/// Whether a publish-platform report may run on KDP or Wide.
fn report_allowed_on_platform(report_id: &str, platform: &str) -> bool {
    match report_id {
        "chapter_summaries" | "genre_analysis" | "genre_ranking" => {
            platform == "kdp" || platform == "wide"
        }
        "kdp_categories" | "kdp_keywords" | "mi_search_terms" | "keyword_search" | "analysis"
        | "competition_report" | "review_mining" | "author_analysis" | "wide_analysis" => {
            platform == "kdp"
        }
        "bisac_classification" | "discovery_keywords" | "google_keyword_search"
        | "content_maturity_advisory" | "wide_metadata_paste" => platform == "kdp",
        _ => false,
    }
}

/// True when `report_id` is selected and allowed on the active platform.
fn wants_report(selected: &[String], report_id: &str, platform: &str) -> bool {
    selected.iter().any(|s| s == report_id) && report_allowed_on_platform(report_id, platform)
}

/// KDP Analysis bundles genre classification, category matching, and keyword optimization.
fn wants_kdp_analysis_bundle(selected: &[String], platform: &str) -> bool {
    wants_report(selected, "analysis", platform)
}

/// Wide Analysis bundles BISAC, discovery keywords, content advisory, and paste sheet.
fn wants_wide_analysis_bundle(selected: &[String], platform: &str) -> bool {
    wants_report(selected, "wide_analysis", platform)
}

fn should_run_genre_analysis(selected: &[String], platform: &str) -> bool {
    wants_report(selected, "genre_analysis", platform)
        || wants_kdp_analysis_bundle(selected, platform)
        || wants_wide_analysis_bundle(selected, platform)
        || wants_report(selected, "bisac_classification", platform)
        || wants_report(selected, "discovery_keywords", platform)
        || wants_report(selected, "google_keyword_search", platform)
        || wants_report(selected, "content_maturity_advisory", platform)
        || wants_report(selected, "wide_metadata_paste", platform)
        || wants_report(selected, "genre_ranking", platform)
}

fn should_run_bisac(selected: &[String], platform: &str, formats: PublishFormats) -> bool {
    if wants_report(selected, "bisac_classification", platform) || wants_wide_analysis_bundle(selected, platform) {
        return formats.ebook || formats.print;
    }
    wants_kdp_analysis_bundle(selected, platform) && formats.print
}

fn should_run_discovery_keywords(selected: &[String], platform: &str, formats: PublishFormats) -> bool {
    (wants_report(selected, "discovery_keywords", platform) || wants_wide_analysis_bundle(selected, platform))
        && formats.ebook
}

fn should_run_content_advisory(selected: &[String], platform: &str, formats: PublishFormats) -> bool {
    (wants_report(selected, "content_maturity_advisory", platform) || wants_wide_analysis_bundle(selected, platform))
        && formats.ebook
}

fn should_run_google_keyword_search(selected: &[String], platform: &str, formats: PublishFormats) -> bool {
    (wants_report(selected, "google_keyword_search", platform) || wants_wide_analysis_bundle(selected, platform))
        && formats.ebook
}

fn should_run_wide_paste(selected: &[String], platform: &str) -> bool {
    wants_report(selected, "wide_metadata_paste", platform) || wants_wide_analysis_bundle(selected, platform)
}

fn should_run_kdp_categories(selected: &[String], platform: &str, formats: PublishFormats) -> bool {
    if !(wants_report(selected, "kdp_categories", platform) || wants_kdp_analysis_bundle(selected, platform)) {
        return false;
    }
    formats.ebook || formats.print
}

fn should_run_kdp_keywords(selected: &[String], platform: &str) -> bool {
    wants_report(selected, "kdp_keywords", platform) || wants_kdp_analysis_bundle(selected, platform)
}

async fn run_bisac_for_formats(
    app: &AppCtx,
    story_id: &str,
    genre_data: &db::GenreDataRow,
    provider: &str,
    api_key: &str,
    model: &str,
    include_ebook: bool,
    include_print: bool,
) -> String {
    let bisac_master = db::master_bisac_list(&app.db.pool).await;
    if bisac_master.is_empty() {
        return String::new();
    }

    let same_as_ebook = genre_data.industry_print.trim().eq_ignore_ascii_case(genre_data.industry_ebook.trim());
    let mut ebook_picks: Vec<(String, String, u8, String)> = Vec::new();

    if include_ebook {
        let ebook_desc = format!("{}\n\n{}", genre_data.industry_ebook, genre_data.genre_signals);
        ebook_picks = ai_pick_bisac(app, story_id, provider, api_key, model, &ebook_desc, &bisac_master)
            .await
            .unwrap_or_default();
        let rows: Vec<(String, String, u8, String)> = ebook_picks
            .iter()
            .map(|(c, h, cf, r)| (c.clone(), h.clone(), *cf, r.clone()))
            .collect();
        let _ = db::replace_bisac_classifications(&app.db.pool, story_id, "ebook", &rows).await;
    }

    let print_picks = if include_print {
        if include_ebook && same_as_ebook {
            let rows: Vec<(String, String, u8, String)> = ebook_picks
                .iter()
                .map(|(c, h, cf, r)| (c.clone(), h.clone(), *cf, r.clone()))
                .collect();
            let _ = db::replace_bisac_classifications(&app.db.pool, story_id, "print", &rows).await;
            None
        } else {
            let print_desc = format!("{}\n\n{}", genre_data.industry_print, genre_data.genre_signals);
            let picks = ai_pick_bisac(app, story_id, provider, api_key, model, &print_desc, &bisac_master)
                .await
                .unwrap_or_default();
            let rows: Vec<(String, String, u8, String)> = picks
                .iter()
                .map(|(c, h, cf, r)| (c.clone(), h.clone(), *cf, r.clone()))
                .collect();
            let _ = db::replace_bisac_classifications(&app.db.pool, story_id, "print", &rows).await;
            Some(picks)
        }
    } else {
        None
    };

    if !include_ebook && include_print {
        if let Some(ref picks) = print_picks {
            ebook_picks = picks.clone();
        }
    }

    serde_json::json!({
        "ebook": if include_ebook {
            serde_json::json!(ebook_picks.iter().map(|(code, heading, conf, reason)| serde_json::json!({
                "code": code, "heading": heading, "confidence": conf, "reason": reason,
            })).collect::<Vec<_>>())
        } else {
            serde_json::json!([])
        },
        "print": match &print_picks {
            None if include_print && include_ebook && same_as_ebook => serde_json::json!("same_as_ebook"),
            None => serde_json::json!([]),
            Some(picks) => serde_json::json!(picks.iter().map(|(code, heading, conf, reason)| serde_json::json!({
                "code": code, "heading": heading, "confidence": conf, "reason": reason,
            })).collect::<Vec<_>>()),
        },
    })
    .to_string()
}

async fn load_ranked_genres(database: &db::Db, story_id: &str) -> Vec<RankedGenre> {
    db::get_genre_rankings(&database.pool, story_id, "Kindle")
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| RankedGenre {
            genre: r.genre,
            confidence: r.confidence as u8,
            reason: r.reason,
            kdp_paths: r.kdp_paths,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn persist_genre_ranking(
    app: &AppCtx,
    database: &db::Db,
    story_id: &str,
    genre_data: &db::GenreDataRow,
    provider: &str,
    api_key: &str,
    model: &str,
    genre_model: &str,
    run_ts: &str,
) -> Result<Vec<RankedGenre>, String> {
    let master_list = crate::genre_taxonomy::master_genre_list(database).await
        .map_err(|e| format!("Could not load genre list from database: {}", e))?;

    let description = format!(
        "{}\n\nKDP paths already identified: {}\n\n{}",
        genre_data.industry_ebook, genre_data.kdp_ebook.join("; "), genre_data.genre_signals
    );

    let ai_ranked = ai_rank_genres(app, story_id, provider, api_key, model, genre_model, &description, &master_list).await?;

    let mut ranked: Vec<RankedGenre> = Vec::new();
    for r in ai_ranked {
        let kdp_paths = crate::genre_taxonomy::kdp_paths_for_genre(database, &r.genre, "Kindle")
            .await
            .unwrap_or_default();
        ranked.push(RankedGenre {
            genre: r.genre,
            confidence: r.confidence,
            reason: r.reason,
            kdp_paths,
        });
    }
    ranked.sort_by(|a, b| b.confidence.cmp(&a.confidence));

    let rows: Vec<(String, u8, String)> = ranked.iter().map(|r| (r.genre.clone(), r.confidence, r.reason.clone())).collect();
    let _ = db::replace_genre_rankings(&database.pool, story_id, &rows).await;

    let ranking_json = serde_json::json!({
        "schema": "genre_ranking_v1",
        "genres": ranked.iter().map(|r| serde_json::json!({
            "genre": r.genre, "confidence": r.confidence, "reason": r.reason,
        })).collect::<Vec<_>>(),
    }).to_string();
    let _ = db::save_document_at(&database.pool, story_id, "genre_ranking", &ranking_json, run_ts).await;

    for r in &ranked {
        emit(app, &format!("  {}% — {}", r.confidence, r.genre));
    }
    Ok(ranked)
}

// ── analyze_story ─────────────────────────────────────────────────────────────

pub async fn analyze_story(app: AppCtx, request: AnalyzeStoryRequest) -> GenreResult {
    let cancel = crate::cancel_notify();
    tokio::select! {
        result = analyze_story_inner(app, request) => result,
        _ = cancel.notified() => err("Cancelled."),
    }
}

async fn analyze_story_inner(app: AppCtx, request: AnalyzeStoryRequest) -> GenreResult {
    let selected = &request.selected;
    // Wide reports are selected via wide_analysis etc. on the KDP tab; remap UI "wide".
    let platform = if request.platform == "wide" { "kdp" } else { request.platform.as_str() };
    let formats = PublishFormats::from_flags(request.publish_ebook, request.publish_print);

    if platform != "kdp" {
        return err("Invalid platform — expected kdp.");
    }
    if selected.is_empty() {
        return err("No reports selected.");
    }
    if request.api_key.is_empty() {
        return err("Platform API keys not configured (Admin).");
    }
    if request.model.is_empty() {
        return err("No model selected. Go to Settings.");
    }
    if !crate::stories::story_exists(&app.db, &request.story_id).await {
        return err("Story not found.");
    }

    let database = app.db.as_ref();
    let run_ts = if request.run_time.is_empty() { chrono::Utc::now().to_rfc3339() } else { request.run_time.clone() };
    let manuscript_fp =
        crate::manuscript_fingerprint::compute_manuscript_fingerprint_for_story(&database.pool, &request.story_id).await;
    let _ = db::sync_manuscript_state(&database.pool, &request.story_id, &manuscript_fp).await;

    let needs_genre_data = should_run_genre_analysis(selected, platform)
        || should_run_kdp_categories(selected, platform, formats)
        || should_run_kdp_keywords(selected, platform)
        || should_run_bisac(selected, platform, formats)
        || wants_report(selected, "mi_search_terms", platform)
        || should_run_discovery_keywords(selected, platform, formats)
        || wants_report(selected, "keyword_search", platform)
        || should_run_google_keyword_search(selected, platform, formats)
        || should_run_content_advisory(selected, platform, formats)
        || wants_kdp_analysis_bundle(selected, platform)
        || wants_wide_analysis_bundle(selected, platform)
        || wants_report(selected, "competition_report", platform)
        || wants_report(selected, "review_mining", platform)
        || wants_report(selected, "author_analysis", platform);

    // ── Step 1: Chapter summaries (infrastructure) ────────────────────────
    if needs_genre_data || request.force_resummarize || wants_report(selected, "chapter_summaries", platform) {
        crate::reset_cancel();
        if request.force_resummarize {
            emit(&app, "  Force re-summarize — deleting existing summaries...");
            let _ = db::delete_chapter_summaries(&database.pool, &request.story_id).await;
        }
        let chapters = documents::list_chapters_db(&app.db, &request.story_id).await.unwrap_or_default();
        if chapters.is_empty() {
            return err("No chapter documents found. Upload manuscript chapters first.");
        }
        let needs_refresh = request.force_resummarize
            || any_chapter_needs_summary(&database.pool, &request.story_id, &chapters).await;
        if needs_refresh {
            emit(&app, "Step 1: Summarizing chapters (AI)...");
            let config = phase1_config_from(
                &request.provider, &request.api_key, &request.model,
                &request.summaries_model, request.force_resummarize,
            );
            let (done, skipped) = phase1_summaries(
                &app, &database, &chapters, &request.story_id, &config,
            ).await;
            emit(&app, &format!("  ✓ {} summarized, {} skipped.", done, skipped));
            let _ = db::record_artifact_built(&database.pool, &request.story_id, "summaries", &manuscript_fp).await;
        } else {
            emit(&app, "Step 1: Chapter summaries up to date — skipping.");
        }
        if wants_report(selected, "chapter_summaries", platform) {
            let summaries = db::load_chapter_summaries(&database.pool, &request.story_id).await;
            if !summaries.is_empty() {
                let cs_json = serde_json::json!({
                    "schema": "chapter_summaries_v1",
                    "chapters": summaries.iter().map(|s| serde_json::json!({
                        "file": s.file, "title": s.title, "signals": s.signals, "word_count": s.word_count,
                    })).collect::<Vec<_>>(),
                    "total_words": summaries.iter().map(|s| s.word_count).sum::<i64>(),
                }).to_string();
                let _ = db::save_document_at(&database.pool, &request.story_id, "chapter_summaries", &cs_json, &run_ts).await;
            }
        }
        if crate::is_cancelled() {
            return err("Cancelled.");
        }
    }

    // ── Step 2: Genre Analysis (+ ranking) ─────────────────────────────────
    if should_run_genre_analysis(selected, platform) {
        emit(&app, "Step 2: Genre analysis...");
        let summaries = db::load_chapter_summaries(&database.pool, &request.story_id).await;
        if summaries.is_empty() {
            return err("No chapter summaries available.");
        }
        let r = phase2_analyze(
            &app, &database, &request.story_id, &summaries,
            &request.provider, &request.api_key, &request.model, &request.genre_model,
        ).await;
        if !r.success {
            return err(&r.error);
        }
        let genre_data = match db::load_genre_data(&database.pool, &request.story_id).await {
            Some(d) => d,
            None => return err("Could not produce genre data."),
        };
        emit(&app, "  Ranking genres against master list...");
        if let Err(e) = persist_genre_ranking(
            &app, &database, &request.story_id, &genre_data,
            &request.provider, &request.api_key, &request.model, &request.genre_model, &run_ts,
        ).await {
            return err(&format!("Genre ranking failed: {}", e));
        }
        let _ = db::record_artifact_built(&database.pool, &request.story_id, "genre_data", &manuscript_fp).await;
        let _ = db::record_artifact_built(&database.pool, &request.story_id, "genre_ranking", &manuscript_fp).await;
        if crate::is_cancelled() {
            return err("Cancelled.");
        }
    }

    let genre_data = if needs_genre_data {
        match db::load_genre_data(&database.pool, &request.story_id).await {
            Some(d) => d,
            None => return err("Genre analysis data is required. Run a positioning report first (KDP Analysis or Wide Analysis)."),
        }
    } else {
        return GenreResult {
            success: true,
            report: String::new(),
            error: String::new(),
            run_ts: run_ts.clone(),
        };
    };

    let ranked = load_ranked_genres(&database, &request.story_id).await;
    let genre_ranking_section = if ranked.is_empty() {
        String::new()
    } else {
        serde_json::json!({
            "genres": ranked.iter().map(|r| serde_json::json!({
                "genre": r.genre,
                "confidence": r.confidence,
                "reason": r.reason,
            })).collect::<Vec<_>>(),
        })
        .to_string()
    };

    let genre_terms: Vec<(String, u8)> = if !ranked.is_empty() {
        ranked.iter().filter(|r| r.confidence >= 30).take(6).map(|r| (r.genre.clone(), r.confidence)).collect()
    } else {
        vec![(genre_data.industry_ebook.clone(), 100)]
    };

    // ── Step 4: KDP Categories ─────────────────────────────────────────────
    let mut kindle_top_categories: Vec<String> = Vec::new();
    let mut print_top_categories: Vec<String> = Vec::new();
    let mut kdp_categories_section = serde_json::json!({ "stores": [] }).to_string();
    if should_run_kdp_categories(selected, platform, formats) {
        emit(&app, "Step 4: Matching KDP categories...");
        let base_description = format!("{}\n\n{}", genre_data.industry_ebook, genre_data.genre_signals);
        let mut kdp_stores_json: Vec<serde_json::Value> = Vec::new();

        let store_jobs: Vec<(&str, &str)> = [
            formats.ebook.then_some(("Kindle", "Kindle eBook")),
            formats.print.then_some(("Books", "Paperback")),
        ]
        .into_iter()
        .flatten()
        .collect();

        for (store, label) in store_jobs {
            let top_cats = if store == "Kindle" { &mut kindle_top_categories } else { &mut print_top_categories };
            let total_catalog = db::kdp_category_count(&database.pool, store).await;
            if total_catalog < 50 {
                kdp_stores_json.push(serde_json::json!({ "store": label, "error": "Catalog nearly empty — import WinningCat data." }));
                continue;
            }

            let result = match_categories_by_store(
                &app, &database, &request.story_id, store, &base_description, &genre_terms,
                &request.provider, &request.api_key, &request.model,
            ).await;

            let final_cats = rank_by_discoverability(&app, store, result.qualifying, &request.canopy_api_key).await;

            for q in final_cats.iter().take(3) {
                top_cats.push(q.path.clone());
            }

            kdp_stores_json.push(serde_json::json!({
                "store": label,
                "categories": final_cats.iter().enumerate().map(|(i, q)| serde_json::json!({
                    "rank": i + 1,
                    "path": q.path,
                    "fit_confidence": q.fit_confidence,
                    "sales_to_ten": q.sales_to_ten,
                    "verified": q.verified,
                    "is_bonus": i >= 3,
                    "agreeing_genres": q.agreeing_genres,
                    "top_books": q.top_books.iter().map(|b| serde_json::json!({
                        "title": b.title,
                        "asin": b.asin,
                        "image_url": b.image_url,
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            }));

            if crate::is_cancelled() { return err("Cancelled."); }
        }
        kdp_categories_section = serde_json::json!({ "stores": kdp_stores_json }).to_string();
        if crate::is_cancelled() { return err("Cancelled."); }
    }

    // ── Step 5: Generate search terms ──────────────────────────────────────
    if wants_report(selected, "mi_search_terms", platform) {
        emit(&app, "Step 5: Generating competition search terms...");
        match generate_mi_search_terms(&app, &request.story_id, &request.provider, &request.api_key, &request.model, &genre_data).await {
            Ok(keywords) => {
                let _ = db::save_mi_search_terms(&database.pool, &request.story_id, &keywords).await;
                let rendered = render_search_terms(&keywords);
                let _ = db::save_document_at(&database.pool, &request.story_id, "mi_search_terms", &rendered, &run_ts).await;
                let _ = db::record_artifact_built(&database.pool, &request.story_id, "mi_search_terms", &manuscript_fp).await;
                emit(&app, &format!("  ✓ {} search terms saved.", keywords.len()));
            }
            Err(e) => emit(&app, &format!("  ⚠ Search terms generation failed: {}", e)),
        }
        if crate::is_cancelled() { return err("Cancelled."); }
    }

    // ── Step 6: BISAC Classification ───────────────────────────────────────
    let mut bisac_section = String::new();
    if should_run_bisac(selected, platform, formats) {
        emit(&app, "Step 6: BISAC classification...");
        let include_ebook = formats.ebook && (
            wants_report(selected, "bisac_classification", platform)
            || wants_wide_analysis_bundle(selected, platform)
        );
        let include_print = formats.print && (
            wants_report(selected, "bisac_classification", platform)
            || wants_wide_analysis_bundle(selected, platform)
            || wants_kdp_analysis_bundle(selected, platform)
        );
        bisac_section = run_bisac_for_formats(
            &app, &request.story_id, &genre_data,
            &request.provider, &request.api_key, &request.model,
            include_ebook, include_print,
        ).await;
        if !bisac_section.is_empty() {
            let _ = db::save_document_at(&database.pool, &request.story_id, "bisac_classification", &bisac_section, &run_ts).await;
            let _ = db::record_artifact_built(&database.pool, &request.story_id, "bisac", &manuscript_fp).await;
        }
        if crate::is_cancelled() { return err("Cancelled."); }
    }

    // ── Step 7: Keyword Search ─────────────────────────────────────────────
    let mut keyword_pool: Vec<KeywordResult> = Vec::new();
    if wants_report(selected, "keyword_search", platform) {
        emit(&app, "Step 7: Keyword search...");
        let top_cats_for_seeds: Vec<String> = kindle_top_categories.iter().take(2).cloned().collect();
        let seeds = derive_keyword_seeds(&genre_data.industry_ebook, &top_cats_for_seeds);
        if seeds.is_empty() {
            emit(&app, "  ⚠ No seeds derived — skipping keyword search.");
        } else {
            emit(&app, &format!("  Seeds: {:?}", seeds));
            keyword_pool = if has_dataforseo_creds(&request.dataforseo_login, &request.dataforseo_password) {
                run_keyword_searches_dataforseo(&app, &request.story_id, &seeds, &request.dataforseo_login, &request.dataforseo_password).await
            } else if !request.canopy_api_key.trim().is_empty() {
                emit(&app, "⚠ Keyword volume API not configured — falling back to alternate keyword source. Add credentials in Admin → Platform credentials.");
                run_keyword_searches_canopy(&app, &request.story_id, &seeds, &request.canopy_api_key).await
            } else {
                emit(&app, "  ⚠ No keyword search credentials — skipping keyword search.");
                Vec::new()
            };
        }
        if !keyword_pool.is_empty() {
            let ks_json = serde_json::json!({
                "schema": "keyword_search_v1",
                "keywords": keyword_pool.iter().map(|k| serde_json::json!({
                    "keyword": k.keyword, "searches": k.searches, "competition": k.competition, "earnings": k.estimated_earnings,
                })).collect::<Vec<_>>(),
            }).to_string();
            let _ = db::save_document_at(&database.pool, &request.story_id, "keyword_search", &ks_json, &run_ts).await;
            let _ = db::record_artifact_built(&database.pool, &request.story_id, "keyword_search", &manuscript_fp).await;
        }
        if crate::is_cancelled() { return err("Cancelled."); }
    }

    // ── Step 8: KDP Keywords ───────────────────────────────────────────────
    let mut kdp_keyword_entries: Vec<db::KdpKeywordEntry> = Vec::new();
    let mut kdp_keyword_strategy = String::new();
    if should_run_kdp_keywords(selected, platform) {
        emit(&app, "Step 8: Optimizing KDP keywords...");
        match call_keyword_optimizer_with_pool(
            &app, &request.story_id, &request.provider, &request.api_key, &request.model,
            &genre_data, &genre_data.genre_signals, &keyword_pool,
        ).await {
            Ok((entries, strategy)) => {
                let source_note = if keyword_pool.is_empty() {
                    "*(Generated from genre analysis — no keyword search data available.)*"
                } else {
                    "*(Enhanced with real Amazon search volume data.)*"
                };
                let _ = db::save_kdp_keywords(&database.pool, &request.story_id, &entries, &strategy, source_note).await;
                emit(&app, &format!("  ✓ {} KDP keyword strings saved.", entries.len()));
                kdp_keyword_entries = entries;
                kdp_keyword_strategy = strategy;
            }
            Err(e) => emit(&app, &format!("  ⚠ KDP keyword optimization failed: {} — continuing.", e)),
        }
        if crate::is_cancelled() { return err("Cancelled."); }
    }

    // ── Step 9: Discovery Keywords ─────────────────────────────────────────
    let mut discovery_entries: Vec<db::DiscoveryKeywordEntry> = Vec::new();
    if should_run_discovery_keywords(selected, platform, formats) {
        emit(&app, "Step 9: Generating discovery keywords...");
        match generate_discovery_keywords(
            &app, &request.story_id, &request.provider, &request.api_key, &request.model, &genre_data,
        ).await {
            Ok(entries) => {
                let enriched = if has_dataforseo_creds(&request.dataforseo_login, &request.dataforseo_password) && !entries.is_empty() {
                    emit(&app, "  Enriching with Google search volume...");
                    let phrases: Vec<String> = entries.iter().map(|e| e.phrase.clone()).collect();
                    let client = crate::dataforseo::DataForSeoClient::new(&request.dataforseo_login, &request.dataforseo_password);
                    match client {
                        Ok(c) => match c.google_search_volume(&phrases).await {
                            Ok(volumes) => {
                                entries.into_iter().map(|mut e| {
                                    if let Some(v) = volumes.iter().find(|v| v.keyword.to_lowercase() == e.phrase.to_lowercase()) {
                                        e.rationale = format!("{}/mo Google — {}", v.search_volume, e.rationale);
                                    }
                                    e
                                }).collect()
                            }
                            Err(err) => { emit(&app, &format!("  ⚠ Search volume lookup failed: {}", err)); entries }
                        }
                        Err(err) => { emit(&app, &format!("  ⚠ Keyword volume API error: {}", err)); entries }
                    }
                } else {
                    entries
                };

                let _ = db::save_discovery_keywords(&database.pool, &request.story_id, &enriched).await;
                let dk_json = serde_json::json!({
                    "schema": "discovery_keywords_v1",
                    "keywords": enriched.iter().map(|e| serde_json::json!({ "phrase": e.phrase, "rationale": e.rationale })).collect::<Vec<_>>(),
                }).to_string();
                let _ = db::save_document_at(&database.pool, &request.story_id, "discovery_keywords", &dk_json, &run_ts).await;
                let _ = db::record_artifact_built(&database.pool, &request.story_id, "discovery_keywords", &manuscript_fp).await;
                emit(&app, &format!("  ✓ {} discovery keywords saved.", enriched.len()));
                discovery_entries = enriched;
            }
            Err(e) => emit(&app, &format!("  ⚠ Discovery keywords failed: {} — continuing.", e)),
        }
        if crate::is_cancelled() { return err("Cancelled."); }
    }

    // ── Step 9b: Google Keyword Search ─────────────────────────────────────
    let mut google_keywords_section = String::new();
    if should_run_google_keyword_search(selected, platform, formats) {
        emit(&app, "Step 9b: Google keyword search...");
        let discovery_phrases: Vec<String> = if !discovery_entries.is_empty() {
            discovery_entries.iter().map(|e| e.phrase.clone()).collect()
        } else {
            db::load_discovery_keywords(&database.pool, &request.story_id)
                .await
                .into_iter()
                .map(|e| e.phrase)
                .collect()
        };
        let seeds = derive_wide_keyword_seeds(&genre_data.industry_ebook, &discovery_phrases);
        if seeds.is_empty() {
            emit(&app, "  ⚠ No seeds derived — skipping Google keyword search.");
        } else if !has_dataforseo_creds(&request.dataforseo_login, &request.dataforseo_password) {
            emit(&app, "  ⚠ DataForSEO credentials not set — add login/password in Settings.");
        } else {
            emit(&app, &format!("  Seeds: {:?}", seeds));
            let results = run_google_keyword_searches_dataforseo(
                &app,
                &request.story_id,
                &seeds,
                &request.dataforseo_login,
                &request.dataforseo_password,
            ).await;
            if !results.is_empty() {
                let ks_json = serde_json::json!({
                    "schema": "google_keyword_search_v1",
                    "keywords": results.iter().map(|k| serde_json::json!({
                        "keyword": k.keyword,
                        "searches": k.searches,
                        "competition": k.competition,
                        "cpc": k.estimated_earnings,
                    })).collect::<Vec<_>>(),
                }).to_string();
                google_keywords_section = ks_json.clone();
                let _ = db::save_document_at(&database.pool, &request.story_id, "google_keyword_search", &ks_json, &run_ts).await;
                let _ = db::record_artifact_built(&database.pool, &request.story_id, "google_keyword_search", &manuscript_fp).await;
                emit(&app, &format!("  ✓ {} Google keywords saved.", results.len()));
            }
        }
        if crate::is_cancelled() { return err("Cancelled."); }
    }

    // ── Step 9c: Content & Maturity Advisory ───────────────────────────────
    let mut content_advisory_section = String::new();
    if should_run_content_advisory(selected, platform, formats) {
        emit(&app, "Step 9c: Content & maturity advisory...");
        let summaries = db::load_chapter_summaries(&database.pool, &request.story_id).await;
        let signals = aggregate_content_signals(&summaries);
        match generate_content_maturity_advisory(
            &app,
            &request.story_id,
            &request.provider,
            &request.api_key,
            &request.model,
            &genre_data,
            &signals,
        ).await {
            Ok(value) => {
                let json = value.to_string();
                content_advisory_section = json.clone();
                let _ = db::save_document_at(
                    &database.pool, &request.story_id, "content_maturity_advisory", &json, &run_ts,
                ).await;
                emit(&app, "  ✓ Content & maturity advisory saved.");
            }
            Err(e) => emit(&app, &format!("  ⚠ Content advisory failed: {} — continuing.", e)),
        }
        if crate::is_cancelled() { return err("Cancelled."); }
    }

    // ── Step 9d: Wide Metadata Paste Sheet ─────────────────────────────────
    let mut wide_paste_section = String::new();
    if should_run_wide_paste(selected, platform) {
        emit(&app, "Step 9d: Assembling wide metadata paste sheet...");
        let ebook_bisac = db::load_bisac_classifications(&database.pool, &request.story_id, "ebook").await;
        let print_bisac = db::load_bisac_classifications(&database.pool, &request.story_id, "print").await;
        let discovery_for_paste = if !discovery_entries.is_empty() {
            discovery_entries.clone()
        } else {
            db::load_discovery_keywords(&database.pool, &request.story_id).await
        };
        let content_note = if !content_advisory_section.is_empty() {
            serde_json::from_str::<serde_json::Value>(&content_advisory_section).ok()
                .and_then(|v| v["maturity_rating_suggestion"].as_str().map(String::from))
        } else {
            db::get_document(&database.pool, &request.story_id, "content_maturity_advisory").await
                .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
                .and_then(|v| v["maturity_rating_suggestion"].as_str().map(String::from))
        };
        if ebook_bisac.is_empty() && print_bisac.is_empty() && discovery_for_paste.is_empty() {
            emit(&app, "  ⚠ BISAC and discovery data not ready — run Wide Analysis or its dependencies.");
        } else {
            wide_paste_section = render_wide_paste_section(
                &genre_data,
                &ebook_bisac,
                &print_bisac,
                &discovery_for_paste,
                content_note.as_deref(),
            );
            if wants_report(selected, "wide_metadata_paste", platform)
                && !wants_wide_analysis_bundle(selected, platform)
            {
                let _ = db::save_document_at(
                    &database.pool, &request.story_id, "wide_metadata_paste", &wide_paste_section, &run_ts,
                ).await;
                emit(&app, "  ✓ Wide metadata paste sheet saved.");
            }
        }
        if crate::is_cancelled() { return err("Cancelled."); }
    }

    // ── Step 9e: Market Intel (KDP — competition, reviews, authors) ────────
    if platform == "kdp" && !request.canopy_api_key.is_empty() {
        if wants_report(selected, "competition_report", platform) {
            emit(&app, "Step 9e: Competition analysis...");
            let result = crate::canopy::analyze_competition_canopy(
                app.clone(),
                crate::canopy::CompetitionCanopyRequest {
                    story_id: request.story_id.clone(),
                    api_key: request.api_key.clone(),
                    model: request.model.clone(),
                    store: "Kindle".to_string(),
                    provider: request.provider.clone(),
                    canopy_api_key: request.canopy_api_key.clone(),
                },
            ).await;
            if result.success {
                if let Some(content) = db::get_document(&database.pool, &request.story_id, "competition_report").await {
                    let _ = db::save_document_at(&database.pool, &request.story_id, "competition_report", &content, &run_ts).await;
                }
                emit(&app, "  ✓ Competition analysis saved.");
            } else {
                emit(&app, &format!("  ⚠ Competition analysis failed: {}", result.error));
            }
        }
        if wants_report(selected, "review_mining", platform) {
            emit(&app, "Step 9e: Review mining...");
            let result = crate::canopy::mine_competitor_reviews(
                app.clone(),
                crate::canopy::ReviewMiningRequest {
                    story_id: request.story_id.clone(),
                    canopy_api_key: request.canopy_api_key.clone(),
                    api_key: request.api_key.clone(),
                    model: request.model.clone(),
                    provider: request.provider.clone(),
                },
            ).await;
            if result.success {
                if let Some(content) = db::get_document(&database.pool, &request.story_id, "review_mining").await {
                    let _ = db::save_document_at(&database.pool, &request.story_id, "review_mining", &content, &run_ts).await;
                }
                emit(&app, "  ✓ Review mining saved.");
            } else {
                emit(&app, &format!("  ⚠ Review mining failed: {}", result.error));
            }
        }
        if wants_report(selected, "author_analysis", platform) {
            emit(&app, "Step 9e: Author analysis...");
            let result = crate::canopy::analyze_comp_authors(
                app.clone(),
                crate::canopy::AuthorAnalysisRequest {
                    story_id: request.story_id.clone(),
                    canopy_api_key: request.canopy_api_key.clone(),
                    api_key: request.api_key.clone(),
                    model: request.model.clone(),
                    provider: request.provider.clone(),
                },
            ).await;
            if result.success {
                if let Some(content) = db::get_document(&database.pool, &request.story_id, "author_analysis").await {
                    let _ = db::save_document_at(&database.pool, &request.story_id, "author_analysis", &content, &run_ts).await;
                }
                emit(&app, "  ✓ Author analysis saved.");
            } else {
                emit(&app, &format!("  ⚠ Author analysis failed: {}", result.error));
            }
        }
        if crate::is_cancelled() { return err("Cancelled."); }
    } else if platform == "kdp" && (
        wants_report(selected, "competition_report", platform)
        || wants_report(selected, "review_mining", platform)
        || wants_report(selected, "author_analysis", platform)
    ) {
        emit(&app, "  ⚠ Canopy API key required for market intel reports — add in Settings.");
    }

    let wants_kdp_bundle = wants_kdp_analysis_bundle(selected, platform);
    let wants_wide_bundle = wants_wide_analysis_bundle(selected, platform);
    if !wants_kdp_bundle && !wants_wide_bundle {
        emit(&app, "✓ Selected reports complete.");
        return GenreResult { success: true, report: String::new(), error: String::new(), run_ts: run_ts.clone() };
    }

    let description_snippet = db::get_document(&database.pool, &request.story_id, "blurb_builder").await
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .and_then(|v| v["variants"].as_array().and_then(|a| a.first()).and_then(|x| x["blurb"].as_str().map(String::from)));

    let discovery_keywords_section = serde_json::json!({
        "keywords": discovery_entries.iter().map(|e| serde_json::json!({
            "phrase": e.phrase,
            "rationale": e.rationale,
        })).collect::<Vec<_>>(),
    }).to_string();

    let positioning_section = serde_json::json!({
        "reader_demographic": genre_data.reader_demographic,
        "bookstore_shelving": genre_data.bookstore_shelving,
        "comps_ebook": genre_data.comps_ebook,
        "comps_print": genre_data.comps_print,
    }).to_string();

    if wants_wide_bundle {
        emit(&app, "Step 10: Assembling Wide Analysis report...");
        if wide_paste_section.is_empty() && should_run_wide_paste(selected, platform) {
            let ebook = db::load_bisac_classifications(&database.pool, &request.story_id, "ebook").await;
            let print = db::load_bisac_classifications(&database.pool, &request.story_id, "print").await;
            let discovery = if !discovery_entries.is_empty() {
                discovery_entries.clone()
            } else {
                db::load_discovery_keywords(&database.pool, &request.story_id).await
            };
            let content_note = if !content_advisory_section.is_empty() {
                serde_json::from_str::<serde_json::Value>(&content_advisory_section).ok()
                    .and_then(|v| v["maturity_rating_suggestion"].as_str().map(String::from))
            } else {
                None
            };
            wide_paste_section = render_wide_paste_section(
                &genre_data, &ebook, &print, &discovery, content_note.as_deref(),
            );
        }
        let report = render_wide_combined_report(
            &genre_ranking_section,
            &bisac_section,
            &discovery_keywords_section,
            &google_keywords_section,
            &content_advisory_section,
            &wide_paste_section,
            &positioning_section,
        );
        let _ = db::save_document_at(&database.pool, &request.story_id, "wide_analysis", &report, &run_ts).await;
        emit(&app, "✓ Wide Analysis report saved.");
        if !wants_kdp_bundle {
            return GenreResult { success: true, report, error: String::new(), run_ts: run_ts.clone() };
        }
    }

    if wants_kdp_bundle {
        emit(&app, "Step 10: Assembling KDP Analysis report...");

        let bisac_print_json = if bisac_section.is_empty() {
            None
        } else {
            serde_json::from_str::<serde_json::Value>(&bisac_section).ok()
                .and_then(|v| if v["print"].is_string() { v.get("ebook").map(|x| x.to_string()) } else { v.get("print").map(|x| x.to_string()) })
        };
        let kdp_paste = render_kdp_paste_section(
            &kindle_top_categories,
            &print_top_categories,
            &kdp_keyword_entries,
            bisac_print_json.as_deref(),
            description_snippet.as_deref(),
        );

        let source_note = if keyword_pool.is_empty() {
            "*(Generated from genre analysis — no keyword search data available.)*"
        } else {
            "*(Enhanced with real Amazon search volume data.)*"
        };
        let kdp_keywords_section = render_kdp_keywords(&kdp_keyword_entries, &kdp_keyword_strategy, source_note);

        let report = render_combined_report(
            &kdp_paste,
            &genre_ranking_section,
            &kdp_categories_section,
            &bisac_section,
            &kdp_keywords_section,
            &discovery_keywords_section,
            &positioning_section,
            description_snippet.as_deref(),
        );

        let _ = db::save_document_at(&database.pool, &request.story_id, "analysis", &report, &run_ts).await;
        emit(&app, "✓ KDP Analysis report saved.");
        return GenreResult { success: true, report, error: String::new(), run_ts: run_ts.clone() };
    }

    GenreResult { success: true, report: String::new(), error: String::new(), run_ts: run_ts.clone() }
}

// ── Paste Section Renderers ───────────────────────────────────────────────────

/// Renders the "KDP Metadata — Ready to Paste" section that mirrors the KDP
/// website's actual input layout.
pub(crate) fn render_kdp_paste_section(
    kindle_categories: &[String],
    print_categories: &[String],
    keywords: &[db::KdpKeywordEntry],
    bisac_print_json: Option<&str>,
    description_snippet: Option<&str>,
) -> String {
    let bisac_print: serde_json::Value = bisac_print_json
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::json!([]));
    let json = serde_json::json!({
        "schema": "kdp_paste_v1",
        "kindle_categories": kindle_categories.iter().take(3).collect::<Vec<_>>(),
        "print_categories": print_categories.iter().take(3).collect::<Vec<_>>(),
        "keywords": keywords.iter().map(|k| &k.string).collect::<Vec<_>>(),
        "bisac_print": bisac_print,
        "description_snippet": description_snippet.unwrap_or(""),
    });
    json.to_string()
}

/// Renders copy-ready wide-distribution metadata (BISAC, discovery keywords, content note).
pub(crate) fn render_wide_paste_section(
    genre_data: &db::GenreDataRow,
    ebook_bisac: &[(String, String, u8, String)],
    print_bisac: &[(String, String, u8, String)],
    discovery: &[db::DiscoveryKeywordEntry],
    content_note: Option<&str>,
) -> String {
    let json = serde_json::json!({
        "schema": "wide_paste_v1",
        "genre_labels": {
            "ebook": genre_data.industry_ebook,
            "print": genre_data.industry_print,
        },
        "bisac_ebook": ebook_bisac.iter().map(|(code, heading, conf, _)| serde_json::json!({
            "code": code,
            "heading": heading,
            "confidence": conf,
        })).collect::<Vec<_>>(),
        "bisac_print": print_bisac.iter().map(|(code, heading, conf, _)| serde_json::json!({
            "code": code,
            "heading": heading,
            "confidence": conf,
        })).collect::<Vec<_>>(),
        "discovery_keywords": discovery.iter().map(|e| &e.phrase).collect::<Vec<_>>(),
        "content_note": content_note.unwrap_or(""),
    });
    json.to_string()
}

// ── Craft pipeline ────────────────────────────────────────────────────────────

/// Request for the craft analysis pipeline.
/// The frontend sends which reports to run; this command handles ordering and execution.
#[derive(serde::Deserialize, serde::Serialize)]
pub struct CraftPipelineRequest {
    #[serde(alias = "folder")]
    pub story_id:           String,
    pub selected:         Vec<String>,
    #[serde(default)]
    pub provider:         String,
    #[serde(default)]
    pub api_key:          String,
    #[serde(default)]
    pub model:            String,           // default fallback
    #[serde(default)]
    pub model_summaries:  String,           // override for chapter summaries
    #[serde(default)]
    pub model_continuity: String,           // override for continuity check
    #[serde(default)]
    pub model_sdt:        String,           // override for show don't tell
    #[serde(default)]
    pub model_ai_isms:    String,           // override for AI-isms check
    /// "manuscript" or "series"
    #[serde(default)]
    pub continuity_scope: String,
    /// Only used when continuity_scope == "series"
    #[serde(default)]
    pub series_id:        i64,
    #[serde(default)]
    pub bible_path:       String,
    #[serde(default)]
    pub model_prose:      String,
}

/// Runs the selected craft-platform reports in the correct order.
/// Chapter summaries → Zeigarnik → Continuity. Each is optional based on `selected`.
pub async fn run_craft_pipeline(app: AppCtx, request: CraftPipelineRequest) -> GenreResult {
    let cancel = crate::cancel_notify();
    tokio::select! {
        result = run_craft_pipeline_inner(app, request) => result,
        _ = cancel.notified() => err("Cancelled."),
    }
}

async fn run_craft_pipeline_inner(app: AppCtx, request: CraftPipelineRequest) -> GenreResult {
    if !crate::stories::story_exists(&app.db, &request.story_id).await {
        return err("Story not found.");
    }

    // Resolve per-function models (fall back to default)
    let model_summaries = if request.model_summaries.is_empty() { &request.model } else { &request.model_summaries };
    let model_continuity = if request.model_continuity.is_empty() { &request.model } else { &request.model_continuity };
    let model_sdt = if request.model_sdt.is_empty() { &request.model } else { &request.model_sdt };
    let model_ai_isms = if request.model_ai_isms.is_empty() { &request.model } else { &request.model_ai_isms };

    crate::reset_cancel();
    let database = app.db.as_ref();
    let run_ts = chrono::Utc::now().to_rfc3339();
    let needs_ai = request.selected.iter().any(|s| {
        s == "chapter_summaries"
            || s == "continuity_check"
            || s == "show_dont_tell"
            || s == "ai_isms"
            || super::craft_audits::is_craft_audit(s)
            || matches!(
                s.as_str(),
                "ai_beta_reader" | "cliffhanger_score" | "hook_strength" | "pacing_curve" | "blurb_builder"
            )
    });

    if needs_ai {
        if let Err(msg) = crate::ai::ai_ready(&request.provider, &request.api_key, &request.model) {
            return err(&msg);
        }
    }

    // ── Chapter Summaries ─────────────────────────────────────────────────
    if request.selected.contains(&"chapter_summaries".to_string()) {
        emit(&app, &format!("Generating chapter summaries... [{}: {}]", request.provider, model_summaries));
        let chapters = documents::list_chapters_db(&app.db, &request.story_id).await.unwrap_or_default();
        if chapters.is_empty() { return err("No chapter documents found. Upload manuscript chapters first."); }

        let config = phase1_config_from(
            &request.provider, &request.api_key, &request.model,
            &request.model_summaries, false,
        );
        let (done, skipped) = phase1_summaries(
            &app, &database, &chapters, &request.story_id, &config,
        ).await;
        emit(&app, &format!("✓ Chapter summaries complete ({} new, {} skipped).", done, skipped));

        // Save as report
        let summaries = db::load_chapter_summaries(&database.pool, &request.story_id).await;
                if !summaries.is_empty() {
                    let cs_json = serde_json::json!({
                        "schema": "chapter_summaries_v1",
                        "chapters": summaries.iter().map(|s| serde_json::json!({
                            "file": s.file, "title": s.title, "signals": s.signals, "word_count": s.word_count,
                        })).collect::<Vec<_>>(),
                        "total_words": summaries.iter().map(|s| s.word_count).sum::<i64>(),
                    }).to_string();
                    let _ = db::save_document_at(&database.pool, &request.story_id, "chapter_summaries", &cs_json, &run_ts).await;
                }
        let manuscript_fp =
            crate::manuscript_fingerprint::compute_manuscript_fingerprint(&chapters);
        let _ = db::record_artifact_built(&database.pool, &request.story_id, "summaries", &manuscript_fp).await;
        if crate::is_cancelled() { return err("Cancelled."); }
    }

    // ── Zeigarnik Effect ──────────────────────────────────────────────────
    if request.selected.contains(&"zeigarnik_analysis".to_string()) {
        emit(&app, "Running Zeigarnik effect analysis (algorithmic — no AI)...");
        let zr = super::zeigarnik::analyze_zeigarnik_for_story(
            app.clone(),
            super::zeigarnik::ZeigarnikRequest { story_id: request.story_id.clone() },
        ).await;
        if zr.success {
            emit(&app, "✓ Zeigarnik analysis complete.");
        } else {
            emit(&app, &format!("✗ Zeigarnik: {}", zr.error));
            return zr;
        }
        if crate::is_cancelled() { return err("Cancelled."); }
    }

    // ── Continuity Check ──────────────────────────────────────────────────
    if request.selected.contains(&"continuity_check".to_string()) {
        if request.continuity_scope == "series" && request.series_id > 0 {
            emit(&app, &format!("Running continuity check across the series... [{}: {}]", request.provider, model_continuity));
            let cr = super::continuity::check_continuity_for_series(
                app.clone(),
                super::continuity::SeriesContinuityRequest {
                    series_id: request.series_id,
                    provider: request.provider.clone(),
                    api_key: request.api_key.clone(),
                    model: model_continuity.clone(),
                    extraction_model: String::new(),
                    bible_path: request.bible_path.clone(),
                },
            ).await;
            if cr.success {
                emit(&app, "✓ Series continuity check complete.");
            } else {
                emit(&app, &format!("✗ Continuity: {}", cr.error));
                return cr;
            }
        } else {
            emit(&app, &format!("Running continuity check for this manuscript... [{}: {}]", request.provider, model_continuity));
            let cr = super::continuity::check_continuity_for_story(
                app.clone(),
                super::continuity::ContinuityRequest {
                    story_id: request.story_id.clone(),
                    provider: request.provider.clone(),
                    api_key: request.api_key.clone(),
                    model: model_continuity.clone(),
                    extraction_model: String::new(),
                    bible_path: request.bible_path.clone(),
                },
            ).await;
            if cr.success {
                emit(&app, "✓ Continuity check complete.");
            } else {
                emit(&app, &format!("✗ Continuity: {}", cr.error));
                return cr;
            }
        }
        if crate::is_cancelled() { return err("Cancelled."); }
    }

    // ── Show Don't Tell + AI-isms (combined when both selected) ───────────
    let wants_sdt = request.selected.contains(&"show_dont_tell".to_string());
    let wants_ai = request.selected.contains(&"ai_isms".to_string());

    if wants_sdt && wants_ai {
        emit(&app, "Running combined show-don't-tell + AI-isms check (single batched pass)...");
        let craft_model = if !request.model_sdt.is_empty() {
            model_sdt.clone()
        } else {
            model_ai_isms.clone()
        };
        let combined = super::craft_prose_checks::check_craft_prose_combined(
            app.clone(),
            super::craft_prose_checks::CraftProseChecksRequest {
                story_id: request.story_id.clone(),
                provider: request.provider.clone(),
                api_key: request.api_key.clone(),
                model: craft_model,
                bible_path: request.bible_path.clone(),
            },
        )
        .await;
        if !combined.success {
            emit(&app, &format!("✗ Craft prose checks: {}", combined.error));
            return combined;
        }
        if crate::is_cancelled() {
            return err("Cancelled.");
        }
    } else if wants_sdt {
        let sdt = super::show_dont_tell::check_show_dont_tell(
            app.clone(),
            super::show_dont_tell::ShowDontTellRequest {
                story_id: request.story_id.clone(),
                provider: request.provider.clone(),
                api_key: request.api_key.clone(),
                model: model_sdt.clone(),
                bible_path: request.bible_path.clone(),
            },
        )
        .await;
        if !sdt.success {
            emit(&app, &format!("✗ Show Don't Tell: {}", sdt.error));
            return sdt;
        }
        if crate::is_cancelled() {
            return err("Cancelled.");
        }
    } else if wants_ai {
        let ai = super::ai_isms::check_ai_isms(
            app.clone(),
            super::ai_isms::AiIsmsRequest {
                story_id: request.story_id.clone(),
                provider: request.provider.clone(),
                api_key: request.api_key.clone(),
                model: model_ai_isms.clone(),
                bible_path: request.bible_path.clone(),
            },
        )
        .await;
        if !ai.success {
            emit(&app, &format!("✗ AI-isms: {}", ai.error));
            return ai;
        }
        if crate::is_cancelled() {
            return err("Cancelled.");
        }
    }

    let model_craft = if request.model_continuity.is_empty() {
        &request.model
    } else {
        &request.model_continuity
    };
    for audit_id in crate::craft_report_groups::manuscript_craft_audit_ids() {
        if !request.selected.iter().any(|s| s == audit_id.as_str()) {
            continue;
        }
        let r = super::craft_audits::run_manuscript_craft_audit(
            &app,
            &database,
            &request.story_id,
            &audit_id,
            &request.provider,
            &request.api_key,
            model_craft,
            &request.bible_path,
        )
        .await;
        if !r.success {
            return r;
        }
        if crate::is_cancelled() {
            return err("Cancelled.");
        }
    }
    let series_id = if request.series_id > 0 {
        request.series_id
    } else {
        0
    };
    for audit_id in crate::craft_report_groups::series_report_ids() {
        if !request.selected.iter().any(|s| s == audit_id.as_str()) {
            continue;
        }
        let r = super::craft_audits::run_series_craft_audit(
            &app,
            &database,
            series_id,
            &audit_id,
            &request.provider,
            &request.api_key,
            model_craft,
            &request.bible_path,
        )
        .await;
        if !r.success {
            return r;
        }
        if crate::is_cancelled() {
            return err("Cancelled.");
        }
    }

    let model_publish = if request.model_summaries.is_empty() {
        &request.model
    } else {
        &request.model_summaries
    };
    let model_prose = if request.model_prose.is_empty() {
        &request.model
    } else {
        &request.model_prose
    };

    if request.selected.iter().any(|s| s == "hook_strength") {
        let r = super::publish_audits::run_hook_strength(
            &app,
            &database,
            &request.story_id,
            &request.provider,
            &request.api_key,
            model_publish,
            &request.bible_path,
        )
        .await;
        if !r.success {
            return r;
        }
        if crate::is_cancelled() {
            return err("Cancelled.");
        }
    }
    if request.selected.iter().any(|s| s == "line_polish") {
        let r = super::publish_audits::run_line_polish(&app, &database, &request.story_id).await;
        if !r.success {
            return r;
        }
    }
    if request.selected.iter().any(|s| s == "blurb_builder") {
        let r = super::publish_audits::run_blurb_builder(
            &app,
            &database,
            &request.story_id,
            &request.provider,
            &request.api_key,
            model_prose,
        )
        .await;
        if !r.success {
            return r;
        }
        if crate::is_cancelled() {
            return err("Cancelled.");
        }
    }
    if request.selected.iter().any(|s| s == "print_production") {
        let r = super::publish_audits::run_print_production(&app, &database, &request.story_id).await;
        if !r.success {
            return r;
        }
    }
    if request.selected.iter().any(|s| s == "ai_beta_reader") {
        let r = super::publish_audits::run_ai_beta_reader(
            &app,
            &database,
            &request.story_id,
            &request.provider,
            &request.api_key,
            model_publish,
            &request.bible_path,
        )
        .await;
        if !r.success {
            return r;
        }
        if crate::is_cancelled() {
            return err("Cancelled.");
        }
    }
    if request.selected.iter().any(|s| s == "cliffhanger_score") {
        let r = super::publish_audits::run_cliffhanger_score(
            &app,
            &database,
            &request.story_id,
            &request.provider,
            &request.api_key,
            model_publish,
        )
        .await;
        if !r.success {
            return r;
        }
        if crate::is_cancelled() {
            return err("Cancelled.");
        }
    }
    if request.selected.iter().any(|s| s == "pacing_curve") {
        let r = super::publish_audits::run_pacing_curve(
            &app,
            &database,
            &request.story_id,
            &request.provider,
            &request.api_key,
            model_publish,
        )
        .await;
        if !r.success {
            return r;
        }
        if crate::is_cancelled() {
            return err("Cancelled.");
        }
    }
    if request.selected.iter().any(|s| s == "vellum_prep") {
        let r = super::publish_audits::run_vellum_prep(&app, &database, &request.story_id).await;
        if !r.success {
            return r;
        }
    }

    emit(&app, "✓ Done.");
    GenreResult { success: true, report: String::new(), error: String::new(), run_ts }
}
