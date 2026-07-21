// db.rs — PostgreSQL-backed storage for the genre/category reference system.
//
// Source of truth for: genres, KDP category paths, genre<->category links,
// and per-story genre rankings / category-finder results. Story registration
// (stories.json) and the human-readable .md reports stay as files — those are
// meant to be read directly outside the app. The DB is queried to produce
// those reports, not the other way around.
//
// The genre-list.json / genre-kdp-map.json files in crates/core/data/ are used
// ONLY as one-time seed data on first launch (when the genres table is
// empty). After that, the database is authoritative — new categories
// discovered via Category Finder get written straight into it, so the
// genre-to-KDP-path map grows on its own with real, verified data instead of
// staying frozen at the hand-typed seed set.

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

const SEED_GENRE_LIST_JSON:    &str = include_str!("../data/genre-list.json");
const SEED_GENRE_KDP_MAP_JSON: &str = include_str!("../data/genre-kdp-map.json");
const SEED_BISAC_JSON:         &str = include_str!("../data/bisac-fiction.json");
const SEED_ZEIGARNIK_CONFIG_JSON: &str = include_str!("../data/zeigarnik-config.json");
const SEED_PROMPT_TEMPLATES_JSON: &str = include_str!("../data/prompt-templates.json");
const SEED_PROVIDER_MODELS_JSON: &str = include_str!("../data/provider-models.json");
const SEED_LOOKUP_CONFIG_JSON: &str = include_str!("../data/lookup-config.json");

pub struct Db(pub PgPool);

/// Connect to PostgreSQL, apply migrations, seed on first run.
pub async fn init(database_url: &str) -> Result<Db, String> {
    let pool = PgPoolOptions::new()
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SET search_path TO lore, public")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| e.to_string())?;

    seed_if_empty(&pool).await?;
    seed_bisac_if_empty(&pool).await?;
    seed_report_types(&pool).await?;
    seed_prompt_templates(&pool).await?;
    seed_zeigarnik_config_if_empty(&pool).await?;
    seed_provider_models(&pool).await?;
    seed_lookup_config(&pool).await?;

    Ok(Db(pool))
}

async fn seed_if_empty(pool: &PgPool) -> Result<(), String> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM genres")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
    if count > 0 { return Ok(()); }

    #[derive(serde::Deserialize)]
    struct SeedGenre { name: String, description: String }

    let genres: Vec<SeedGenre> = serde_json::from_str(SEED_GENRE_LIST_JSON)
        .map_err(|e| format!("Cannot parse seed genre-list.json: {}", e))?;
    let kdp_map: std::collections::HashMap<String, Vec<String>> =
        serde_json::from_str(SEED_GENRE_KDP_MAP_JSON)
            .map_err(|e| format!("Cannot parse seed genre-kdp-map.json: {}", e))?;

    let now = chrono::Utc::now().to_rfc3339();

    for g in &genres {
        sqlx::query(
            "INSERT INTO genres (name, description) VALUES ($1, $2)
             ON CONFLICT (name) DO NOTHING",
        )
        .bind(&g.name)
        .bind(&g.description)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

        let genre_id: i64 = sqlx::query_scalar("SELECT id FROM genres WHERE name = $1")
            .bind(&g.name)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;

        if let Some(paths) = kdp_map.get(&g.name) {
            for path in paths {
                sqlx::query(
                    "INSERT INTO kdp_categories (path, store, source, created_at)
                     VALUES ($1, 'Kindle', 'manual', $2)
                     ON CONFLICT (path, store) DO NOTHING",
                )
                .bind(path)
                .bind(&now)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;

                let category_id: i64 = sqlx::query_scalar(
                    "SELECT id FROM kdp_categories WHERE path = $1 AND store = 'Kindle'",
                )
                .bind(path)
                .fetch_one(pool)
                .await
                .map_err(|e| e.to_string())?;

                sqlx::query(
                    "INSERT INTO genre_kdp_links (genre_id, category_id) VALUES ($1, $2)
                     ON CONFLICT (genre_id, category_id) DO NOTHING",
                )
                .bind(genre_id)
                .bind(category_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}

async fn seed_bisac_if_empty(pool: &PgPool) -> Result<(), String> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bisac_codes")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
    if count > 0 { return Ok(()); }

    #[derive(serde::Deserialize)]
    struct SeedBisac { code: String, heading: String }

    let codes: Vec<SeedBisac> = serde_json::from_str(SEED_BISAC_JSON)
        .map_err(|e| format!("Cannot parse seed bisac-fiction.json: {}", e))?;

    for c in &codes {
        sqlx::query(
            "INSERT INTO bisac_codes (code, heading) VALUES ($1, $2)
             ON CONFLICT (code) DO NOTHING",
        )
        .bind(&c.code)
        .bind(&c.heading)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn seed_report_types(pool: &PgPool) -> Result<(), String> {
    // (id, label, description, platforms, depends_on,
    //  cost_truncation, cost_output_max, cost_per_chapter, cost_fixed_calls, model_slot, min_tier)
    let rows: &[(&str, &str, &str, &str, &str, i64, i64, i64, i64, &str, &str)] = &[
        ("chapter_summaries", "Chapter Summaries", "Extract genre signals from each chapter of the manuscript.", "kdp,wide,craft", "", 8000, 600, 1, 0, "summaries", "basic"),
        ("genre_analysis", "Genre Analysis", "Industry genre classification, KDP paths, comps, and reader demographic.", "kdp,wide", "chapter_summaries", 0, 1200, 0, 1, "genre", "capable"),
        ("genre_ranking", "Genre Ranking", "Score the manuscript against all known genres independently.", "kdp,wide", "chapter_summaries,genre_analysis", 0, 1200, 0, 1, "genre", "capable"),
        ("kdp_categories", "KDP Categories", "Find the best-fit Amazon categories with discoverability stats.", "kdp", "chapter_summaries,genre_analysis,genre_ranking", 0, 1200, 0, 2, "keywords", "basic"),
        ("kdp_keywords", "KDP Keywords", "Optimize the 7 keyword strings for KDP discoverability.", "kdp", "chapter_summaries,genre_analysis,genre_ranking", 0, 1200, 0, 1, "keywords", "basic"),
        ("bisac_classification", "BISAC Classification", "Select BISAC subject codes for KDP Print and Ingram distribution.", "kdp,wide", "chapter_summaries,genre_analysis", 0, 1200, 0, 2, "keywords", "basic"),
        ("mi_search_terms", "Search Terms", "Generate competition search phrases for market analysis.", "kdp", "chapter_summaries,genre_analysis", 0, 300, 0, 1, "keywords", "basic"),
        ("discovery_keywords", "Discovery Keywords", "Keywords optimized for Apple Books, Kobo, Google Play, and SEO.", "wide", "chapter_summaries,genre_analysis", 0, 1200, 0, 1, "keywords", "basic"),
        ("analysis", "Full Analysis", "Combined report: categories, BISAC, keywords, and positioning all in one.", "kdp", "chapter_summaries,genre_analysis,genre_ranking,kdp_categories,kdp_keywords,bisac_classification,mi_search_terms", 4000, 1000, 0, 1, "default", "basic"),
        ("keyword_search", "Keyword Search Results", "Amazon keyword volume and competition data from DataForSEO.", "kdp", "chapter_summaries,genre_analysis,genre_ranking", 4000, 1000, 0, 1, "keywords", "basic"),
        ("competition_report", "Competition Analysis", "Market landscape: how competitive the niche is, who dominates.", "kdp", "mi_search_terms", 4000, 1000, 0, 1, "default", "basic"),
        ("review_mining", "Reader Review Intelligence", "Reader insights extracted from competitor book reviews.", "kdp", "mi_search_terms", 4000, 1000, 0, 1, "default", "basic"),
        ("author_analysis", "Competitor Author Analysis", "Competitor pricing, release cadence, and series strategy.", "kdp", "mi_search_terms", 4000, 1000, 0, 1, "default", "basic"),
        ("zeigarnik_analysis", "Zeigarnik Effect", "Analyzes open loops and unresolved tension to maintain reader engagement.", "craft", "", 0, 0, 0, 0, "default", "basic"),
        ("continuity_check", "Continuity Check", "AI-assisted scan for contradicted facts — within a manuscript or across a whole series.", "craft", "", 6000, 4000, 1, 3, "continuity", "capable"),
        ("show_dont_tell", "Show Don't Tell", "AI-assisted check for telling instead of showing — flags violations with surrounding manuscript text.", "craft", "", 4000, 4000, 1, 0, "showDontTell", "capable"),
        ("ai_isms", "AI-isms", "AI-assisted check for prose habits that often read as machine-generated — flags passages with surrounding manuscript text.", "craft", "", 4000, 4000, 1, 0, "aiIsms", "capable"),
    ];

    for (id, label, description, platforms, depends_on, trunc, out_max, per_ch, fixed, slot, tier) in rows {
        sqlx::query(
            "INSERT INTO report_types (
                id, label, description, platforms, depends_on,
                cost_truncation, cost_output_max, cost_per_chapter, cost_fixed_calls, model_slot, min_tier
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT(id) DO UPDATE SET
                label = excluded.label,
                description = excluded.description,
                platforms = excluded.platforms,
                depends_on = excluded.depends_on,
                cost_truncation = excluded.cost_truncation,
                cost_output_max = excluded.cost_output_max,
                cost_per_chapter = excluded.cost_per_chapter,
                cost_fixed_calls = excluded.cost_fixed_calls,
                model_slot = excluded.model_slot,
                min_tier = excluded.min_tier",
        )
        .bind(id)
        .bind(label)
        .bind(description)
        .bind(platforms)
        .bind(depends_on)
        .bind(trunc)
        .bind(out_max)
        .bind(per_ch)
        .bind(fixed)
        .bind(slot)
        .bind(tier)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn seed_prompt_templates(pool: &PgPool) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct SeedPrompt {
        id: String,
        label: String,
        system_prompt: String,
        user_template: String,
        max_tokens: i64,
        json_mode: i64,
    }

    let templates: Vec<SeedPrompt> = serde_json::from_str(SEED_PROMPT_TEMPLATES_JSON)
        .map_err(|e| format!("Cannot parse seed prompt-templates.json: {}", e))?;

    // Dev app: always refresh from seed so prompt edits ship with the build.
    sqlx::query("DELETE FROM prompt_templates")
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    for t in &templates {
        sqlx::query(
            "INSERT INTO prompt_templates (id, label, system_prompt, user_template, max_tokens, json_mode, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, to_char(NOW() AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'))",
        )
        .bind(&t.id)
        .bind(&t.label)
        .bind(&t.system_prompt)
        .bind(&t.user_template)
        .bind(t.max_tokens)
        .bind(t.json_mode)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn seed_provider_models(pool: &PgPool) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct SeedModel {
        id: String,
        provider: String,
        owned_by: String,
        input_price: Option<f64>,
        output_price: Option<f64>,
        sort_order: i64,
    }

    let models: Vec<SeedModel> = serde_json::from_str(SEED_PROVIDER_MODELS_JSON)
        .map_err(|e| format!("Cannot parse seed provider-models.json: {}", e))?;

    sqlx::query("DELETE FROM provider_models")
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    for m in &models {
        sqlx::query(
            "INSERT INTO provider_models (id, provider, owned_by, input_price, output_price, sort_order)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&m.id)
        .bind(&m.provider)
        .bind(&m.owned_by)
        .bind(m.input_price)
        .bind(m.output_price)
        .bind(m.sort_order)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn seed_lookup_config(pool: &PgPool) -> Result<(), String> {
    let parsed: serde_json::Value = serde_json::from_str(SEED_LOOKUP_CONFIG_JSON)
        .map_err(|e| format!("Cannot parse seed lookup-config.json: {}", e))?;
    let obj = parsed.as_object().ok_or("lookup-config.json must be a JSON object")?;

    sqlx::query("DELETE FROM lookup_config")
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    for (key, value) in obj {
        sqlx::query("INSERT INTO lookup_config (key, value) VALUES ($1, $2)")
            .bind(key)
            .bind(value.to_string())
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn seed_zeigarnik_config_if_empty(pool: &PgPool) -> Result<(), String> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM zeigarnik_config")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
    if count > 0 { return Ok(()); }

    let parsed: serde_json::Value = serde_json::from_str(SEED_ZEIGARNIK_CONFIG_JSON)
        .map_err(|e| format!("Cannot parse seed zeigarnik-config.json: {}", e))?;

    let obj = parsed.as_object().ok_or("zeigarnik-config.json must be a JSON object")?;
    for (key, value) in obj {
        if key == "thresholds" {
            // Flatten thresholds into individual keys so each is independently tunable.
            if let Some(t) = value.as_object() {
                for (tkey, tval) in t {
                    sqlx::query(
                        "INSERT INTO zeigarnik_config (key, value) VALUES ($1, $2)
                         ON CONFLICT (key) DO NOTHING",
                    )
                    .bind(format!("threshold.{}", tkey))
                    .bind(tval.to_string())
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                }
            }
        } else {
            sqlx::query(
                "INSERT INTO zeigarnik_config (key, value) VALUES ($1, $2)
                 ON CONFLICT (key) DO NOTHING",
            )
            .bind(key)
            .bind(value.to_string())
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// ── Zeigarnik effect detector (craft platform, no AI) ──────────────────────

#[derive(Clone, Debug)]
pub struct ZeigarnikConfig {
    pub cliffhanger_markers:  Vec<String>,
    pub resolution_markers:   Vec<String>,
    pub question_lead_ins:    Vec<String>,
    pub short_fragment_max_words:      usize,
    pub min_gap_chapters_for_thread:   usize,
    pub max_total_mentions_for_thread: usize,
    pub min_thread_term_len:           usize,
    pub top_threads_limit:             usize,
    pub min_question_words:            usize,
    pub max_questions_per_chapter:     usize,
}

impl Default for ZeigarnikConfig {
    fn default() -> Self {
        ZeigarnikConfig {
            cliffhanger_markers: vec![], resolution_markers: vec![], question_lead_ins: vec![],
            short_fragment_max_words: 8, min_gap_chapters_for_thread: 3,
            max_total_mentions_for_thread: 6, min_thread_term_len: 4,
            top_threads_limit: 25, min_question_words: 4, max_questions_per_chapter: 6,
        }
    }
}

/// Load the Zeigarnik phrase lists and thresholds from the database. Falls
/// back to sane defaults for any key missing (e.g. a fresh DB where seeding
/// somehow failed) rather than erroring the whole analysis out.
pub async fn load_zeigarnik_config(pool: &PgPool) -> ZeigarnikConfig {
    let mut cfg = ZeigarnikConfig::default();

    async fn get_str_list(pool: &PgPool, key: &str) -> Vec<String> {
        sqlx::query_scalar::<_, String>("SELECT value FROM zeigarnik_config WHERE key = $1")
            .bind(key)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .unwrap_or_default()
    }
    async fn get_usize(pool: &PgPool, key: &str, default: usize) -> usize {
        sqlx::query_scalar::<_, String>("SELECT value FROM zeigarnik_config WHERE key = $1")
            .bind(format!("threshold.{}", key))
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(default)
    }

    cfg.cliffhanger_markers = get_str_list(pool, "cliffhanger_markers").await;
    cfg.resolution_markers  = get_str_list(pool, "resolution_markers").await;
    cfg.question_lead_ins   = get_str_list(pool, "question_lead_ins").await;
    cfg.short_fragment_max_words      = get_usize(pool, "short_fragment_max_words", 8).await;
    cfg.min_gap_chapters_for_thread   = get_usize(pool, "min_gap_chapters_for_thread", 3).await;
    cfg.max_total_mentions_for_thread = get_usize(pool, "max_total_mentions_for_thread", 6).await;
    cfg.min_thread_term_len           = get_usize(pool, "min_thread_term_len", 4).await;
    cfg.top_threads_limit             = get_usize(pool, "top_threads_limit", 25).await;
    cfg.min_question_words            = get_usize(pool, "min_question_words", 4).await;
    cfg.max_questions_per_chapter     = get_usize(pool, "max_questions_per_chapter", 6).await;

    cfg
}

#[derive(Clone, Debug)]
pub struct ZeigarnikChapterRow {
    pub chapter_index:  i64,
    pub file:           String,
    pub title:          String,
    pub word_count:     i64,
    pub sentence_count: i64,
    pub question_count: i64,
    pub ending_type:    String,
    pub tension_score:  i64,
    pub ending_snippet: String,
}

#[derive(Clone, Debug)]
pub struct ZeigarnikThreadRow {
    pub term:                String,
    pub mention_count:       i64,
    pub first_chapter_index: i64,
    pub first_file:          String,
    pub first_snippet:       String,
    pub gap_start_index:     i64,
    pub gap_end_index:       i64,
    pub max_gap_chapters:    i64,
    pub max_gap_words:       i64,
}

/// Replace all stored Zeigarnik chapter metrics + threads for a story with a
/// fresh set — same "latest run supersedes" model used everywhere else.
pub async fn replace_zeigarnik_analysis(
    pool: &PgPool,
    story_id: &str,
    chapters: &[ZeigarnikChapterRow],
    threads: &[ZeigarnikThreadRow],
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("DELETE FROM zeigarnik_chapters WHERE story_id = $1")
        .bind(story_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM zeigarnik_threads WHERE story_id = $1")
        .bind(story_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    for c in chapters {
        sqlx::query(
            "INSERT INTO zeigarnik_chapters
             (story_id, chapter_index, file, title, word_count, sentence_count, question_count, ending_type, tension_score, ending_snippet, generated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(story_id)
        .bind(c.chapter_index)
        .bind(&c.file)
        .bind(&c.title)
        .bind(c.word_count)
        .bind(c.sentence_count)
        .bind(c.question_count)
        .bind(&c.ending_type)
        .bind(c.tension_score)
        .bind(&c.ending_snippet)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    for t in threads {
        sqlx::query(
            "INSERT INTO zeigarnik_threads
             (story_id, term, mention_count, first_chapter_index, first_file, first_snippet, gap_start_index, gap_end_index, max_gap_chapters, max_gap_words, generated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(story_id)
        .bind(&t.term)
        .bind(t.mention_count)
        .bind(t.first_chapter_index)
        .bind(&t.first_file)
        .bind(&t.first_snippet)
        .bind(t.gap_start_index)
        .bind(t.gap_end_index)
        .bind(t.max_gap_chapters)
        .bind(t.max_gap_words)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub async fn has_zeigarnik_analysis(pool: &PgPool, story_id: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM zeigarnik_chapters WHERE story_id = $1)",
    )
    .bind(story_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

// ── Series (Continuity Checker: grouping stories in reading order) ────────────

#[derive(serde::Serialize, Clone, Debug)]
pub struct SeriesRow {
    pub id:         i64,
    pub name:       String,
    pub book_count: i64,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct SeriesBookRow {
    pub story_id: String,
    pub story_name:   String,
    pub book_order:   i64,
}

pub async fn list_series(pool: &PgPool) -> Result<Vec<SeriesRow>, String> {
    let rows = sqlx::query(
        r#"SELECT s.id, s.name, COUNT(sb.story_id)
         FROM "series" s LEFT JOIN series_books sb ON sb.series_id = s.id
         GROUP BY s.id ORDER BY s.name"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    rows.into_iter()
        .map(|r| {
            Ok(SeriesRow {
                id: r.try_get(0).map_err(|e| e.to_string())?,
                name: r.try_get(1).map_err(|e| e.to_string())?,
                book_count: r.try_get(2).map_err(|e| e.to_string())?,
            })
        })
        .collect()
}

pub async fn create_series(pool: &PgPool, name: &str) -> Result<SeriesRow, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let id: i64 = sqlx::query_scalar(
        r#"INSERT INTO "series" (name, created_at) VALUES ($1, $2) RETURNING id"#,
    )
    .bind(name.trim())
    .bind(&now)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(SeriesRow { id, name: name.trim().to_string(), book_count: 0 })
}

/// Deletes the series and its book memberships. Does NOT delete the stories
/// themselves or any continuity data already recorded under the series key.
pub async fn delete_series(pool: &PgPool, series_id: i64) -> Result<(), String> {
    sqlx::query("DELETE FROM series_books WHERE series_id = $1")
        .bind(series_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query(r#"DELETE FROM "series" WHERE id = $1"#)
        .bind(series_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn list_series_books(pool: &PgPool, series_id: i64) -> Result<Vec<SeriesBookRow>, String> {
    let rows = sqlx::query(
        "SELECT story_id, story_name, book_order FROM series_books
         WHERE series_id = $1 ORDER BY book_order",
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    rows.into_iter()
        .map(|r| {
            Ok(SeriesBookRow {
                story_id: r.try_get(0).map_err(|e| e.to_string())?,
                story_name: r.try_get(1).map_err(|e| e.to_string())?,
                book_order: r.try_get(2).map_err(|e| e.to_string())?,
            })
        })
        .collect()
}

pub async fn add_story_to_series(pool: &PgPool, series_id: i64, story_id: &str, story_name: &str, book_order: i64) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO series_books (series_id, story_id, story_name, book_order) VALUES ($1, $2, $3, $4)
         ON CONFLICT(series_id, story_id) DO UPDATE SET story_name = excluded.story_name, book_order = excluded.book_order",
    )
    .bind(series_id)
    .bind(story_id)
    .bind(story_name)
    .bind(book_order)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn remove_story_from_series(pool: &PgPool, series_id: i64, story_id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM series_books WHERE series_id = $1 AND story_id = $2")
        .bind(series_id)
        .bind(story_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn list_series_cmd(db: &Db) -> Result<Vec<SeriesRow>, String> {
    list_series(&db.0).await
}

pub async fn create_series_cmd(db: &Db, name: String) -> Result<SeriesRow, String> {
    if name.trim().is_empty() { return Err("Series name cannot be empty.".to_string()); }
    create_series(&db.0, &name).await
}

pub async fn delete_series_cmd(db: &Db, series_id: i64) -> Result<(), String> {
    delete_series(&db.0, series_id).await
}

pub async fn list_series_books_cmd(db: &Db, series_id: i64) -> Result<Vec<SeriesBookRow>, String> {
    list_series_books(&db.0, series_id).await
}

#[derive(serde::Deserialize)]
pub struct AddToSeriesRequest {
    pub series_id:    i64,
    pub story_id: String,
    pub story_name:   String,
    pub book_order:   i64,
}

pub async fn add_story_to_series_cmd(db: &Db, request: AddToSeriesRequest) -> Result<(), String> {
    add_story_to_series(&db.0, request.series_id, &request.story_id, &request.story_name, request.book_order).await
}

pub async fn remove_story_from_series_cmd(db: &Db, series_id: i64, story_id: String) -> Result<(), String> {
    remove_story_from_series(&db.0, series_id, &story_id).await
}

// ── Continuity Checker (craft platform, AI-assisted) ────────────────────

#[derive(Clone, Debug)]
pub struct ContinuityFactRow {
    pub chapter_index: i64,
    pub file:          String,
    pub chapter_title: String,
    pub entity:        String,
    pub entity_type:   String,
    pub attribute:     String,
    pub value:         String,
    pub snippet:       String,
}

pub async fn replace_continuity_facts(pool: &PgPool, story_id: &str, facts: &[ContinuityFactRow]) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("DELETE FROM continuity_facts WHERE story_id = $1")
        .bind(story_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    for f in facts {
        sqlx::query(
            "INSERT INTO continuity_facts
             (story_id, chapter_index, file, chapter_title, entity, entity_type, attribute, value, snippet, generated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(story_id)
        .bind(f.chapter_index)
        .bind(&f.file)
        .bind(&f.chapter_title)
        .bind(&f.entity)
        .bind(&f.entity_type)
        .bind(&f.attribute)
        .bind(&f.value)
        .bind(&f.snippet)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ContinuityOccurrence {
    pub story_id:  String,
    pub story_name:    String,
    pub file:          String,
    pub chapter_title: String,
    pub chapter_index: i64,
    pub value:         String,
    pub snippet:       String,
}

#[derive(Clone, Debug)]
pub struct ContinuityFindingRow {
    pub entity:       String,
    pub attribute:    String,
    pub verdict:      String,
    pub confidence:   i64,
    pub explanation:  String,
    pub occurrences:  Vec<ContinuityOccurrence>,
}

/// Replace all stored findings for a scope (one manuscript, or one series) —
/// same "latest run supersedes" model used everywhere else.
pub async fn replace_continuity_findings(pool: &PgPool, scope: &str, scope_key: &str, findings: &[ContinuityFindingRow]) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("DELETE FROM continuity_findings WHERE scope = $1 AND scope_key = $2")
        .bind(scope)
        .bind(scope_key)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    for f in findings {
        let occ_json = serde_json::to_string(&f.occurrences).unwrap_or_default();
        sqlx::query(
            "INSERT INTO continuity_findings
             (scope, scope_key, entity, attribute, verdict, confidence, explanation, occurrences_json, generated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(scope)
        .bind(scope_key)
        .bind(&f.entity)
        .bind(&f.attribute)
        .bind(&f.verdict)
        .bind(f.confidence)
        .bind(&f.explanation)
        .bind(occ_json)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── Tauri commands (for future UI — browsing/editing the genre/category map) ───────────

pub async fn list_genres_cmd(db: &Db) -> Result<Vec<GenreRow>, String> {
    list_genres(&db.0).await
}

#[derive(serde::Deserialize)]
pub struct AddKdpPathRequest {
    pub genre_name: String,
    pub path:       String,
    pub store:      String,
}

pub async fn add_kdp_path_cmd(db: &Db, request: AddKdpPathRequest) -> Result<(), String> {
    upsert_kdp_path(&db.0, &request.genre_name, &request.path, &request.store, "manual", false).await
}

// ── Query helpers used by genre_analyzer.rs / category_finder.rs ──────────────

#[derive(serde::Serialize, Clone, Debug)]
pub struct GenreRow {
    pub id:          i64,
    pub name:        String,
    pub description: String,
}

pub async fn list_genres(pool: &PgPool) -> Result<Vec<GenreRow>, String> {
    let rows = sqlx::query("SELECT id, name, description FROM genres ORDER BY name")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    rows.into_iter()
        .map(|r| {
            Ok(GenreRow {
                id: r.try_get(0).map_err(|e| e.to_string())?,
                name: r.try_get(1).map_err(|e| e.to_string())?,
                description: r.try_get::<Option<String>, _>(2).map_err(|e| e.to_string())?.unwrap_or_default(),
            })
        })
        .collect()
}

/// Get every known KDP path for a genre name (by exact name match).
pub async fn kdp_paths_for_genre(pool: &PgPool, genre_name: &str, store: &str) -> Result<Vec<String>, String> {
    let rows = sqlx::query(
        "SELECT kc.path FROM kdp_categories kc
         JOIN genre_kdp_links gkl ON gkl.category_id = kc.id
         JOIN genres g ON g.id = gkl.genre_id
         WHERE g.name = $1 AND kc.store = $2
         ORDER BY kc.verified_at DESC NULLS LAST",
    )
    .bind(genre_name)
    .bind(store)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    rows.into_iter()
        .map(|r| r.try_get(0).map_err(|e| e.to_string()))
        .collect()
}

/// Record (or update) a KDP category path and link it to a genre. Used both
/// for manual corrections and for auto-growth from Category Finder results.
/// Marks the path as verified (sets verified_at) when `verified` is true —
/// i.e. when it came from a live, successful category lookup.
pub async fn upsert_kdp_path(
    pool: &PgPool,
    genre_name: &str,
    path: &str,
    store: &str,
    source: &str,
    verified: bool,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let verified_at: Option<String> = if verified { Some(now.clone()) } else { None };

    sqlx::query(
        "INSERT INTO kdp_categories (path, store, source, verified_at, created_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT(path, store) DO UPDATE SET
            verified_at = CASE WHEN $4 IS NOT NULL THEN $4 ELSE kdp_categories.verified_at END,
            source = excluded.source",
    )
    .bind(path)
    .bind(store)
    .bind(source)
    .bind(&verified_at)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    let category_id: i64 = sqlx::query_scalar(
        "SELECT id FROM kdp_categories WHERE path = $1 AND store = $2",
    )
    .bind(path)
    .bind(store)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT INTO genres (name, description) VALUES ($1, '')
         ON CONFLICT (name) DO NOTHING",
    )
    .bind(genre_name)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    let genre_id: i64 = sqlx::query_scalar("SELECT id FROM genres WHERE name = $1")
        .bind(genre_name)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT INTO genre_kdp_links (genre_id, category_id) VALUES ($1, $2)
         ON CONFLICT (genre_id, category_id) DO NOTHING",
    )
    .bind(genre_id)
    .bind(category_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Replace all stored genre rankings for a story with a fresh set — "latest
/// ranking wins" rather than accumulating history, since re-running Rank
/// Genres means the previous ranking is superseded, not a separate data point.
pub async fn replace_genre_rankings(
    pool: &PgPool,
    story_id: &str,
    rankings: &[(String, u8, String)],  // (genre_name, confidence, reason)
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("DELETE FROM genre_rankings WHERE story_id = $1")
        .bind(story_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    for (genre_name, confidence, reason) in rankings {
        sqlx::query(
            "INSERT INTO genres (name, description) VALUES ($1, '')
             ON CONFLICT (name) DO NOTHING",
        )
        .bind(genre_name)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
        let genre_id: i64 = sqlx::query_scalar("SELECT id FROM genres WHERE name = $1")
            .bind(genre_name)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;

        sqlx::query(
            "INSERT INTO genre_rankings (story_id, genre_id, confidence, reason, generated_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(story_id)
        .bind(genre_id)
        .bind(*confidence as i32)
        .bind(reason)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct RankingRow {
    pub genre:      String,
    pub confidence: i64,
    pub reason:     String,
    pub kdp_paths:  Vec<String>,
}

pub async fn get_genre_rankings(pool: &PgPool, story_id: &str, store: &str) -> Result<Vec<RankingRow>, String> {
    let rows = sqlx::query(
        "SELECT g.name, gr.confidence, gr.reason
         FROM genre_rankings gr JOIN genres g ON g.id = gr.genre_id
         WHERE gr.story_id = $1
         ORDER BY gr.confidence DESC",
    )
    .bind(story_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for r in rows {
        let genre: String = r.try_get(0).map_err(|e| e.to_string())?;
        let confidence: i64 = r.try_get(1).map_err(|e| e.to_string())?;
        let reason: String = r.try_get(2).map_err(|e| e.to_string())?;
        let kdp_paths = kdp_paths_for_genre(pool, &genre, store).await?;
        out.push(RankingRow { genre, confidence, reason, kdp_paths });
    }
    Ok(out)
}

pub async fn has_genre_rankings(pool: &PgPool, story_id: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM genre_rankings WHERE story_id = $1)",
    )
    .bind(story_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

pub async fn has_category_results(pool: &PgPool, story_id: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM category_results WHERE story_id = $1)",
    )
    .bind(story_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

pub async fn kdp_category_count(pool: &PgPool, store: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM kdp_categories WHERE store = $1")
        .bind(store)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

/// Keyword search over the imported category catalog — case-insensitive
/// substring match per term, deduplicated, capped at `limit`. This is the
/// direct replacement for Category Finder's live top-level scraping: once
/// the catalog is populated (WinningCat import, or prior discoveries), this
/// is a plain SQL query instead of scraping any external UI at all.
pub async fn search_kdp_categories(pool: &PgPool, store: &str, terms: &[String], limit: usize) -> Vec<(String, String)> {
    if terms.is_empty() { return Vec::new(); }
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();

    for term in terms {
        let cleaned = term.replace('%', " ").replace('_', " ");
        let cleaned = cleaned.trim();
        if cleaned.is_empty() { continue; }
        let pattern = format!("%{}%", cleaned);

        let rows = match sqlx::query(
            "SELECT path, COALESCE(amazon_node_id,'') FROM kdp_categories
             WHERE store = $1 AND path ILIKE $2 LIMIT 200",
        )
        .bind(store)
        .bind(&pattern)
        .fetch_all(pool)
        .await {
            Ok(r) => r,
            Err(_) => continue,
        };

        for row in rows {
            let path: String = row.try_get(0).unwrap_or_default();
            let node_id: String = row.try_get(1).unwrap_or_default();
            if seen.insert(path.clone()) {
                out.push((path, node_id));
                if out.len() >= limit { return out; }
            }
        }
    }
    out
}

/// Import a category path + node ID from an external catalog (WinningCat)
/// without linking it to any genre yet — that happens later via Category
/// Finder discovery or manual mapping. Preserves the source label if a path
/// was already verified live (category_finder /
/// category_analyzer outrank a catalog import), but always refreshes the
/// node ID and last_seen_at since those are authoritative either way.
pub async fn import_kdp_category(pool: &PgPool, path: &str, store: &str, node_id: &str) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO kdp_categories (path, store, amazon_node_id, source, created_at, last_seen_at)
         VALUES ($1, $2, $3, 'winningcat', $4, $4)
         ON CONFLICT(path, store) DO UPDATE SET
            amazon_node_id = excluded.amazon_node_id,
            last_seen_at = $4,
            source = CASE
                WHEN kdp_categories.source IN ('category_finder', 'category_analyzer')
                THEN kdp_categories.source ELSE 'winningcat' END",
    )
    .bind(path)
    .bind(store)
    .bind(node_id)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Every catalog entry sourced from WinningCat that was NOT touched by an
/// import run started at or after `since` — i.e. it was in a previous
/// WinningCat file but missing from the latest one. Doesn't delete anything
/// automatically (Amazon renaming a category and it genuinely disappearing
/// look identical from here); surfaces the list so a human decides.
pub async fn stale_winningcat_paths(pool: &PgPool, since: &str) -> Vec<(String, String)> {
    sqlx::query(
        "SELECT path, store FROM kdp_categories
         WHERE source = 'winningcat' AND (last_seen_at IS NULL OR last_seen_at < $1)
         ORDER BY store, path",
    )
    .bind(since)
    .fetch_all(pool)
    .await
    .ok()
    .map(|rows| {
        rows.into_iter()
            .filter_map(|r| {
                let path: String = r.try_get(0).ok()?;
                let store: String = r.try_get(1).ok()?;
                Some((path, store))
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Remove every WinningCat-sourced catalog entry not seen since `since`.
/// Called only when the user explicitly confirms cleanup after reviewing
/// the stale count from an import — not automatic.
pub async fn remove_stale_winningcat_paths(pool: &PgPool, since: &str) -> Result<usize, String> {
    let result = sqlx::query(
        "DELETE FROM kdp_categories WHERE source = 'winningcat' AND (last_seen_at IS NULL OR last_seen_at < $1)",
    )
    .bind(since)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(result.rows_affected() as usize)
}

/// Replace all stored category-finder results for a story with a fresh set.
/// Every matched/considered result also gets written into kdp_categories and
/// linked to the genre it was found under (when it clears 80%, marked
/// verified — this is how the genre->KDP map grows from real usage).
pub async fn replace_category_results(
    pool: &PgPool,
    story_id: &str,
    store: &str,
    top_genre_hint: Option<&str>,
    results: &[(String, u8, String, String, String, String, String, Option<String>)],
    // (path, confidence, sales_to_one, sales_to_ten, publisher_pct, ku_pct, status, note)
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("DELETE FROM category_results WHERE story_id = $1")
        .bind(story_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    for (path, confidence, sales_to_one, sales_to_ten, publisher_pct, ku_pct, status, note) in results {
        let category_id: Option<i64> = if status != "failed" {
            let _ = sqlx::query(
                "INSERT INTO kdp_categories (path, store, source, verified_at, created_at)
                 VALUES ($1, $2, 'category_finder', $3, $3)
                 ON CONFLICT(path, store) DO UPDATE SET verified_at = $3, source = 'category_finder'",
            )
            .bind(path)
            .bind(store)
            .bind(&now)
            .execute(pool)
            .await;

            let id: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM kdp_categories WHERE path = $1 AND store = $2",
            )
            .bind(path)
            .bind(store)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

            if let (Some(cat_id), Some(genre_name)) = (id, top_genre_hint) {
                let _ = sqlx::query(
                    "INSERT INTO genres (name, description) VALUES ($1, '')
                     ON CONFLICT (name) DO NOTHING",
                )
                .bind(genre_name)
                .execute(pool)
                .await;

                if let Ok(genre_id) = sqlx::query_scalar::<_, i64>("SELECT id FROM genres WHERE name = $1")
                    .bind(genre_name)
                    .fetch_one(pool)
                    .await
                {
                    let _ = sqlx::query(
                        "INSERT INTO genre_kdp_links (genre_id, category_id) VALUES ($1, $2)
                         ON CONFLICT (genre_id, category_id) DO NOTHING",
                    )
                    .bind(genre_id)
                    .bind(cat_id)
                    .execute(pool)
                    .await;
                }
            }
            id
        } else {
            None
        };

        sqlx::query(
            "INSERT INTO category_results
             (story_id, category_id, raw_path, store, confidence, sales_to_one, sales_to_ten,
              publisher_pct, ku_pct, status, note, generated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(story_id)
        .bind(category_id)
        .bind(path)
        .bind(store)
        .bind(*confidence as i32)
        .bind(sales_to_one)
        .bind(sales_to_ten)
        .bind(publisher_pct)
        .bind(ku_pct)
        .bind(status)
        .bind(note)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

// ── Chapter summaries ──────────────────────────────────────────────────

#[derive(serde::Serialize, Clone, Debug)]
pub struct ChapterSummaryRow {
    pub file:       String,
    pub title:      String,
    pub signals:    String,
    pub word_count: i64,
}

pub async fn chapter_summary_exists(pool: &PgPool, story_id: &str, file: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM chapter_summaries WHERE story_id = $1 AND file = $2)",
    )
    .bind(story_id)
    .bind(file)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

pub async fn save_chapter_summary(
    pool: &PgPool, story_id: &str, file: &str, title: &str, signals: &str, word_count: i64,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO chapter_summaries (story_id, file, title, signals, word_count, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT(story_id, file) DO UPDATE SET
            title = excluded.title, signals = excluded.signals,
            word_count = excluded.word_count, updated_at = excluded.updated_at",
    )
    .bind(story_id)
    .bind(file)
    .bind(title)
    .bind(signals)
    .bind(word_count)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn load_chapter_summaries(pool: &PgPool, story_id: &str) -> Vec<ChapterSummaryRow> {
    sqlx::query(
        "SELECT file, title, signals, word_count FROM chapter_summaries
         WHERE story_id = $1 ORDER BY file",
    )
    .bind(story_id)
    .fetch_all(pool)
    .await
    .ok()
    .map(|rows| {
        rows.into_iter()
            .filter_map(|r| {
                Some(ChapterSummaryRow {
                    file: r.try_get(0).ok()?,
                    title: r.try_get(1).ok()?,
                    signals: r.try_get(2).ok()?,
                    word_count: r.try_get(3).ok()?,
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

pub async fn chapter_summary_count(pool: &PgPool, story_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM chapter_summaries WHERE story_id = $1")
        .bind(story_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

/// Wipe all chapter summaries for a story so the next Analyze run
/// regenerates every chapter from scratch, instead of skipping ones that
/// already have a summary. Used by the "force re-summarize" checkbox.
pub async fn delete_chapter_summaries(pool: &PgPool, story_id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM chapter_summaries WHERE story_id = $1")
        .bind(story_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Genre classification (industry genre + KDP paths + comps + notes) ──────────────

#[derive(Clone, Debug)]
pub struct GenreDataRow {
    pub industry_ebook:     String,
    pub industry_print:     String,
    pub genre_signals:      String,
    pub reader_demographic: String,
    pub bookstore_shelving: String,
    pub kdp_ebook:          Vec<String>,
    pub kdp_print:          Vec<String>,
    pub comps_ebook:        Vec<String>,
    pub comps_print:        Vec<String>,
    pub marketing_notes:    Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub async fn save_genre_data(
    pool: &PgPool,
    story_id: &str,
    industry_ebook: &str,
    industry_print: &str,
    genre_signals: &str,
    reader_demographic: &str,
    bookstore_shelving: &str,
    kdp_ebook: &[String],
    kdp_print: &[String],
    comps_ebook: &[String],
    comps_print: &[String],
    marketing_notes: &[String],
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO genre_data
         (story_id, generated_at, industry_ebook, industry_print, genre_signals,
          reader_demographic, bookstore_shelving, kdp_ebook_json, kdp_print_json,
          comps_ebook_json, comps_print_json, marketing_notes_json)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         ON CONFLICT(story_id) DO UPDATE SET
            generated_at = excluded.generated_at,
            industry_ebook = excluded.industry_ebook,
            industry_print = excluded.industry_print,
            genre_signals = excluded.genre_signals,
            reader_demographic = excluded.reader_demographic,
            bookstore_shelving = excluded.bookstore_shelving,
            kdp_ebook_json = excluded.kdp_ebook_json,
            kdp_print_json = excluded.kdp_print_json,
            comps_ebook_json = excluded.comps_ebook_json,
            comps_print_json = excluded.comps_print_json,
            marketing_notes_json = excluded.marketing_notes_json",
    )
    .bind(story_id)
    .bind(&now)
    .bind(industry_ebook)
    .bind(industry_print)
    .bind(genre_signals)
    .bind(reader_demographic)
    .bind(bookstore_shelving)
    .bind(serde_json::to_string(kdp_ebook).unwrap_or_default())
    .bind(serde_json::to_string(kdp_print).unwrap_or_default())
    .bind(serde_json::to_string(comps_ebook).unwrap_or_default())
    .bind(serde_json::to_string(comps_print).unwrap_or_default())
    .bind(serde_json::to_string(marketing_notes).unwrap_or_default())
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn load_genre_data(pool: &PgPool, story_id: &str) -> Option<GenreDataRow> {
    let row = sqlx::query(
        "SELECT industry_ebook, industry_print, genre_signals, reader_demographic,
                bookstore_shelving, kdp_ebook_json, kdp_print_json, comps_ebook_json,
                comps_print_json, marketing_notes_json
         FROM genre_data WHERE story_id = $1",
    )
    .bind(story_id)
    .fetch_optional(pool)
    .await
    .ok()??;

    let parse = |s: String| serde_json::from_str::<Vec<String>>(&s).unwrap_or_default();
    Some(GenreDataRow {
        industry_ebook:     row.try_get(0).ok()?,
        industry_print:     row.try_get(1).ok()?,
        genre_signals:      row.try_get(2).ok()?,
        reader_demographic: row.try_get(3).ok()?,
        bookstore_shelving: row.try_get(4).ok()?,
        kdp_ebook:          parse(row.try_get(5).ok()?),
        kdp_print:          parse(row.try_get(6).ok()?),
        comps_ebook:        parse(row.try_get(7).ok()?),
        comps_print:        parse(row.try_get(8).ok()?),
        marketing_notes:    parse(row.try_get(9).ok()?),
    })
}

// ── KDP keywords (the 7 ready-to-paste strings) ──────────────────────────

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct KdpKeywordEntry {
    pub string:    String,
    pub chars:     i64,
    pub rationale: String,
}

pub async fn save_kdp_keywords(
    pool: &PgPool, story_id: &str, keywords: &[KdpKeywordEntry], strategy: &str, source_note: &str,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO kdp_keywords (story_id, generated_at, keywords_json, strategy, source_note)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT(story_id) DO UPDATE SET
            generated_at = excluded.generated_at, keywords_json = excluded.keywords_json,
            strategy = excluded.strategy, source_note = excluded.source_note",
    )
    .bind(story_id)
    .bind(&now)
    .bind(serde_json::to_string(keywords).unwrap_or_default())
    .bind(strategy)
    .bind(source_note)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn load_kdp_keywords(pool: &PgPool, story_id: &str) -> Option<(Vec<KdpKeywordEntry>, String, String)> {
    let row = sqlx::query(
        "SELECT keywords_json, strategy, source_note FROM kdp_keywords WHERE story_id = $1",
    )
    .bind(story_id)
    .fetch_optional(pool)
    .await
    .ok()??;

    let json: String = row.try_get(0).ok()?;
    let strategy: Option<String> = row.try_get(1).ok()?;
    let note: Option<String> = row.try_get(2).ok()?;
    let keywords: Vec<KdpKeywordEntry> = serde_json::from_str(&json).unwrap_or_default();
    Some((keywords, strategy.unwrap_or_default(), note.unwrap_or_default()))
}

// ── MI search-term keywords ─────────────────────────────────────────

pub async fn save_mi_search_terms(pool: &PgPool, story_id: &str, keywords: &[String]) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO mi_search_terms (story_id, generated_at, keywords_json)
         VALUES ($1, $2, $3)
         ON CONFLICT(story_id) DO UPDATE SET generated_at = excluded.generated_at, keywords_json = excluded.keywords_json",
    )
    .bind(story_id)
    .bind(&now)
    .bind(serde_json::to_string(keywords).unwrap_or_default())
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn load_mi_search_terms(pool: &PgPool, story_id: &str) -> Vec<String> {
    sqlx::query_scalar::<_, String>("SELECT keywords_json FROM mi_search_terms WHERE story_id = $1")
        .bind(story_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

// ── Non-KDP discovery keywords (broader platforms: Apple Books, Kobo, etc.) ──

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DiscoveryKeywordEntry {
    pub phrase:    String,
    pub rationale: String,
}

pub async fn save_discovery_keywords(pool: &PgPool, story_id: &str, entries: &[DiscoveryKeywordEntry]) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO discovery_keywords (story_id, generated_at, keywords_json)
         VALUES ($1, $2, $3)
         ON CONFLICT(story_id) DO UPDATE SET generated_at = excluded.generated_at, keywords_json = excluded.keywords_json",
    )
    .bind(story_id)
    .bind(&now)
    .bind(serde_json::to_string(entries).unwrap_or_default())
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn load_discovery_keywords(pool: &PgPool, story_id: &str) -> Vec<DiscoveryKeywordEntry> {
    sqlx::query_scalar::<_, String>("SELECT keywords_json FROM discovery_keywords WHERE story_id = $1")
        .bind(story_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

pub async fn has_keyword_search_results(pool: &PgPool, story_id: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM keyword_search_results WHERE story_id = $1)",
    )
    .bind(story_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

// ── Keyword Search results (real search volume / competition) ──

/// Replace all stored results for this story+seed — latest search wins, same
/// "supersede, don't accumulate" model used everywhere else in this app.
pub async fn replace_keyword_search_results(
    pool: &PgPool, story_id: &str, seed: &str,
    rows: &[(String, String, String, String)],  // (keyword, searches, competition, earnings)
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("DELETE FROM keyword_search_results WHERE story_id = $1 AND seed = $2")
        .bind(story_id)
        .bind(seed)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    for (keyword, searches, competition, earnings) in rows {
        sqlx::query(
            "INSERT INTO keyword_search_results (story_id, seed, keyword, searches, competition, earnings, generated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(story_id)
        .bind(seed)
        .bind(keyword)
        .bind(searches)
        .bind(competition)
        .bind(earnings)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}


// ── Story documents (rendered markdown cache, read by the Reports panel) ─────

/// Look up the display label for a doc_type from the report_types table.
/// Falls back to the doc_type string itself if not found.
async fn label_for_doc_type(pool: &PgPool, doc_type: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT label FROM report_types WHERE id = $1")
        .bind(doc_type)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| doc_type.to_string())
}

pub async fn save_document(pool: &PgPool, story_id: &str, doc_type: &str, content: &str) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    save_document_at(pool, story_id, doc_type, content, &now).await
}

pub async fn save_document_at(pool: &PgPool, story_id: &str, doc_type: &str, content: &str, timestamp: &str) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO story_documents (story_id, doc_type, content, generated_at)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(story_id)
    .bind(doc_type)
    .bind(content)
    .bind(timestamp)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn get_document(pool: &PgPool, story_id: &str, doc_type: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT content FROM story_documents WHERE story_id = $1 AND doc_type = $2 ORDER BY generated_at DESC LIMIT 1",
    )
    .bind(story_id)
    .bind(doc_type)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct DocMeta {
    pub id:           i64,
    pub doc_type:     String,
    pub label:        String,
    pub generated_at: String,
}

/// Response envelope for get_report_cmd — tells the frontend what format to expect.
#[derive(serde::Serialize, Clone, Debug)]
pub struct ReportEnvelope {
    pub id:           i64,
    pub doc_type:     String,
    pub label:        String,
    pub format:       String,   // "json" | "markdown"
    pub content:      String,
    pub generated_at: String,
}

pub async fn list_documents(pool: &PgPool, story_id: &str) -> Vec<DocMeta> {
    let rows = sqlx::query(
        "SELECT id, doc_type, generated_at FROM story_documents WHERE story_id = $1 ORDER BY generated_at DESC",
    )
    .bind(story_id)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    let mut out = Vec::new();
    for r in rows {
        let id: i64 = r.try_get(0).unwrap_or(0);
        let doc_type: String = r.try_get(1).unwrap_or_default();
        let generated_at: String = r.try_get(2).unwrap_or_default();
        let label = label_for_doc_type(pool, &doc_type).await;
        out.push(DocMeta { id, doc_type, label, generated_at });
    }
    out
}

// ── Tauri commands for the Reports panel ────────────────────────────

#[derive(serde::Serialize, Clone, Debug)]
pub struct ReportTypeDef {
    pub id:          String,
    pub label:       String,
    pub description: String,
    pub platforms:   Vec<String>,
    pub depends_on:  Vec<String>,
    pub model_slot:  String,
    pub min_tier:    String,
}

#[derive(Clone, Debug)]
pub struct ReportCostParams {
    pub truncation:  usize,
    pub output_max:  usize,
    pub per_chapter: bool,
    pub fixed_calls: usize,
}

impl Default for ReportCostParams {
    fn default() -> Self {
        Self { truncation: 4000, output_max: 1000, per_chapter: false, fixed_calls: 1 }
    }
}

/// Load cost-estimate parameters for a report type. Falls back to defaults if missing.
pub async fn load_report_cost_params(pool: &PgPool, report_id: &str) -> ReportCostParams {
    sqlx::query(
        "SELECT cost_truncation, cost_output_max, cost_per_chapter, cost_fixed_calls
         FROM report_types WHERE id = $1",
    )
    .bind(report_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|r| ReportCostParams {
        truncation:  r.try_get::<i32, _>(0).unwrap_or(4000) as usize,
        output_max:  r.try_get::<i32, _>(1).unwrap_or(1000) as usize,
        per_chapter: r.try_get::<i32, _>(2).unwrap_or(0) != 0,
        fixed_calls: r.try_get::<i32, _>(3).unwrap_or(1) as usize,
    })
    .unwrap_or_default()
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct ProviderModelRow {
    pub id: String,
    pub owned_by: String,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
}

pub async fn list_provider_models(pool: &PgPool, provider: &str) -> Vec<ProviderModelRow> {
    sqlx::query(
        "SELECT id, owned_by, input_price, output_price FROM provider_models
         WHERE provider = $1 ORDER BY sort_order ASC, id ASC",
    )
    .bind(provider)
    .fetch_all(pool)
    .await
    .ok()
    .map(|rows| {
        rows.into_iter()
            .filter_map(|r| {
                Some(ProviderModelRow {
                    id: r.try_get(0).ok()?,
                    owned_by: r.try_get(1).ok()?,
                    input_price: r.try_get(2).ok()?,
                    output_price: r.try_get(3).ok()?,
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Load a JSON string-array from lookup_config. Returns empty vec if missing/invalid.
pub async fn load_lookup_string_list(pool: &PgPool, key: &str) -> Vec<String> {
    sqlx::query_scalar::<_, String>("SELECT value FROM lookup_config WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}

pub async fn list_report_types_cmd(db: &Db) -> Result<Vec<ReportTypeDef>, String> {
    let rows = sqlx::query(
        "SELECT id, label, description, platforms, depends_on, model_slot, min_tier
         FROM report_types ORDER BY id",
    )
    .fetch_all(&db.0)
    .await
    .map_err(|e| e.to_string())?;

    rows.into_iter()
        .map(|r| {
            let platforms: String = r.try_get(3).map_err(|e| e.to_string())?;
            let depends_on: String = r.try_get(4).map_err(|e| e.to_string())?;
            Ok(ReportTypeDef {
                id:          r.try_get(0).map_err(|e| e.to_string())?,
                label:       r.try_get(1).map_err(|e| e.to_string())?,
                description: r.try_get(2).map_err(|e| e.to_string())?,
                platforms:   platforms.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                depends_on:  depends_on.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                model_slot:  r.try_get(5).map_err(|e| e.to_string())?,
                min_tier:    r.try_get(6).map_err(|e| e.to_string())?,
            })
        })
        .collect()
}

pub async fn list_reports_cmd(db: &Db, folder: String) -> Result<Vec<DocMeta>, String> {
    Ok(list_documents(&db.0, &folder).await)
}

pub async fn save_activity_log_cmd(db: &Db, folder: String, content: String, timestamp: String) -> Result<(), String> {
    let ts = if timestamp.is_empty() { chrono::Utc::now().to_rfc3339() } else { timestamp };
    save_document_at(&db.0, &folder, "activity_log", &content, &ts).await
}

pub async fn get_report_cmd(db: &Db, id: i64) -> Result<ReportEnvelope, String> {
    let row = sqlx::query(
        "SELECT doc_type, content, generated_at FROM story_documents WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&db.0)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Report not found.".to_string())?;

    let doc_type: String = row.try_get(0).map_err(|e| e.to_string())?;
    let content: String = row.try_get(1).map_err(|e| e.to_string())?;
    let generated_at: String = row.try_get(2).map_err(|e| e.to_string())?;

    let label = label_for_doc_type(&db.0, &doc_type).await;

    let format = if content.starts_with('{') || content.starts_with('[') {
        if serde_json::from_str::<serde_json::Value>(&content).is_ok() { "json" } else { "markdown" }
    } else {
        "markdown"
    };

    Ok(ReportEnvelope { id, doc_type, label, format: format.to_string(), content, generated_at })
}

// ── Delete a report version ────────────────────────

pub async fn delete_report_cmd(db: &Db, id: i64) -> Result<(), String> {
    let result = sqlx::query("DELETE FROM story_documents WHERE id = $1")
        .bind(id)
        .execute(&db.0)
        .await
        .map_err(|e| e.to_string())?;
    if result.rows_affected() == 0 { return Err("Report not found.".to_string()); }
    Ok(())
}

// ── Sidebar data (grouped reports by platform) ─────────────────────────

#[derive(serde::Serialize, Clone, Debug)]
pub struct SidebarReportVersion {
    pub id:           i64,
    pub generated_at: String,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct SidebarReportGroup {
    pub doc_type:    String,
    pub label:       String,
    pub description: String,
    pub count:       usize,
    pub versions:    Vec<SidebarReportVersion>,
}

/// Returns reports grouped by type, filtered by platform, sorted newest-first.
/// This is the single source of truth for the sidebar's report list.
pub async fn get_sidebar_reports(db: &Db, folder: String, platform: String) -> Result<Vec<SidebarReportGroup>, String> {
    let all_types: Vec<(String, String, String)> = sqlx::query(
        "SELECT id, label, description FROM report_types ORDER BY id",
    )
    .fetch_all(&db.0)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .filter_map(|r| {
        Some((
            r.try_get(0).ok()?,
            r.try_get(1).ok()?,
            r.try_get(2).ok()?,
        ))
    })
    .collect();

    let plat_rows = sqlx::query("SELECT id, platforms FROM report_types")
        .fetch_all(&db.0)
        .await
        .map_err(|e| e.to_string())?;

    let plat_map: std::collections::HashMap<String, Vec<String>> = plat_rows
        .into_iter()
        .filter_map(|r| {
            let id: String = r.try_get(0).ok()?;
            let platforms: String = r.try_get(1).ok()?;
            let plats: Vec<String> = platforms.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            Some((id, plats))
        })
        .collect();

    let docs = list_documents(&db.0, &folder).await;

    let mut versions_by_type: std::collections::HashMap<String, Vec<SidebarReportVersion>> = std::collections::HashMap::new();
    for doc in &docs {
        versions_by_type.entry(doc.doc_type.clone()).or_default().push(SidebarReportVersion {
            id: doc.id,
            generated_at: doc.generated_at.clone(),
        });
    }

    let groups: Vec<SidebarReportGroup> = all_types.into_iter()
        .filter(|(id, _, _)| {
            plat_map.get(id).map(|p| p.contains(&platform)).unwrap_or(false)
        })
        .map(|(id, label, description)| {
            let versions = versions_by_type.remove(&id).unwrap_or_default();
            let count = versions.len();
            SidebarReportGroup { doc_type: id, label, description, count, versions }
        })
        .collect();

    Ok(groups)
}

// ── BISAC classifications ──────────────────────────────────────────────

#[derive(serde::Serialize, Clone, Debug)]
pub struct BisacCodeRow {
    pub code:    String,
    pub heading: String,
}

pub async fn master_bisac_list(pool: &PgPool) -> Vec<BisacCodeRow> {
    sqlx::query("SELECT code, heading FROM bisac_codes ORDER BY code")
        .fetch_all(pool)
        .await
        .ok()
        .map(|rows| {
            rows.into_iter()
                .filter_map(|r| {
                    Some(BisacCodeRow {
                        code: r.try_get(0).ok()?,
                        heading: r.try_get(1).ok()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Replace all stored BISAC classifications for a story+format — latest call
/// wins, same "supersede, don't accumulate" model as genre rankings. `format`
/// is "ebook" or "print", scored and stored independently since a print-only
/// distribution can legitimately warrant different codes than the ebook.
pub async fn replace_bisac_classifications(
    pool: &PgPool, story_id: &str, format: &str, rows: &[(String, String, u8, String)],
    // (code, heading, confidence, reason)
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("DELETE FROM bisac_classifications WHERE story_id = $1 AND format = $2")
        .bind(story_id)
        .bind(format)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    for (code, heading, confidence, reason) in rows {
        sqlx::query(
            "INSERT INTO bisac_classifications (story_id, code, heading, confidence, reason, generated_at, format)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(story_id)
        .bind(code)
        .bind(heading)
        .bind(*confidence as i32)
        .bind(reason)
        .bind(&now)
        .bind(format)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub async fn has_bisac_classifications(pool: &PgPool, story_id: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM bisac_classifications WHERE story_id = $1)",
    )
    .bind(story_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

// ── Top-level KDP categories (derived from catalog) ─────────────────────

/// Look up the Amazon node ID for a category path. Returns None if the path
/// isn't in the catalog or has no node ID (manually-added paths without WinningCat data).
pub async fn node_id_for_path(pool: &PgPool, path: &str, store: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT amazon_node_id FROM kdp_categories WHERE path = $1 AND store = $2 AND amazon_node_id IS NOT NULL AND amazon_node_id != ''",
    )
    .bind(path)
    .bind(store)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}
