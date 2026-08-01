// analysis/continuity.rs — AI-assisted continuity checker.
//
// Unlike Zeigarnik (deliberately pattern-only), spotting a contradicted fact
// — an eye color that changes, a character's age that doesn't track, a
// timeline that doesn't add up — requires understanding what a sentence
// *means*, not just how it's shaped. So this module uses the LLM for two
// distinct passes:
//
//   1. Extraction — batched calls across chapters, pulling out
//      continuity-relevant facts (who/what/where/when) as structured data.
//      Cheap, deterministic in shape even if not in content.
//   2. Judgment — after cheap, non-AI pre-filtering narrows facts down to
//      only entity+attribute groups with more than one distinct recorded
//      value, the LLM judges each surviving group: genuine contradiction,
//      or an explainable change (aging, injury, disguise, a plot twist)?
//
// Works at two scopes: a single manuscript's chapters, or every book in a
// series in reading order (see db.rs `series` / `series_books`).

use std::collections::HashMap;

use super::{emit, err, GenreResult};
use super::chapters::{extract_title, truncate_words, CONTINUITY_EXCERPT_WORD_LIMIT};
use crate::app_ctx::AppCtx;
use crate::batch_prompt::{self, BatchChapterItem, CachedBatchItem, DEFAULT_WORD_BUDGET};
use crate::db;
use crate::documents;
use crate::manuscript_fingerprint;
use crate::prompts::BibleTier;

// ── Requests ─────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct ContinuityRequest {
    #[serde(alias = "folder")]
    pub story_id:   String,
    pub provider: String,
    pub api_key:  String,
    pub model:    String,
    #[serde(default)]
    pub extraction_model: String,
    #[serde(default)]
    pub bible_path: String,
}

#[derive(serde::Deserialize)]
pub struct SeriesContinuityRequest {
    pub series_id: i64,
    pub provider:  String,
    pub api_key:   String,
    pub model:     String,
    #[serde(default)]
    pub extraction_model: String,
    #[serde(default)]
    pub bible_path: String,
}

// ── Internal working types ──────────────────────────────────────────────────

struct ChapterFacts {
    chapter_index: usize,
    file:          String,
    title:         String,
    facts:         Vec<AiFact>,
}

struct Book {
    story_id: String,
    story_name:   String,
    chapters:     Vec<ChapterFacts>,
}

#[derive(serde::Deserialize, Clone, Debug)]
struct AiFact {
    #[serde(default)]
    entity:      String,
    #[serde(default = "default_entity_type")]
    entity_type: String,
    #[serde(default)]
    attribute:   String,
    #[serde(default)]
    value:       String,
    #[serde(default)]
    snippet:     String,
}

fn default_entity_type() -> String { "other".to_string() }

// ── Manuscript-scope command ────────────────────────────────────────────────

pub async fn check_continuity_for_story(app: AppCtx, request: ContinuityRequest) -> GenreResult {
    if !crate::stories::story_exists(&app.db, &request.story_id).await {
        return err("Story not found.");
    }
    if let Err(msg) = crate::ai::ai_ready(&request.provider, &request.api_key, &request.model) {
        return err(&msg);
    }
    crate::reset_cancel();

    let database = app.db.as_ref();
    let story_name = sqlx::query_scalar::<_, String>("SELECT name FROM stories WHERE id = $1")
        .bind(&request.story_id)
        .fetch_optional(&database.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| request.story_id.clone());
    let bible = crate::prompts::load_bible_tiered(&app.db, &request.story_id, &request.bible_path, BibleTier::Medium).await;
    let extraction_model = crate::ai::resolve_slot_model(&request.extraction_model, &request.model)
        .unwrap_or_else(|_| request.model.clone());

    let book = match extract_book_facts(&app, &database, &request.story_id, &story_name, &request.provider, &request.api_key, &extraction_model, &bible).await {
        Ok(b) => b,
        Err(e) => return err(&e),
    };

    let _ = db::replace_continuity_facts(&database.pool, &request.story_id, &flatten_facts(&book)).await;

    emit(&app, "Comparing facts across chapters for contradictions...");
    let findings = match judge_contradictions(&app, &database, &request.provider, &request.api_key, &request.model, &[book], &bible).await {
        Ok(f) => f,
        Err(e) => return err(&e),
    };
    emit(&app, &format!("  {} finding(s) worth reviewing.", findings.len()));

    let run_ts = chrono::Utc::now().to_rfc3339();
    let _ = db::replace_continuity_findings(&database.pool, "manuscript", &request.story_id, &findings).await;

    let content = render_findings_json(&findings, "manuscript", &request.story_id);
    let _ = db::save_document_at(&database.pool, &request.story_id, "continuity_check", &content, &run_ts).await;
    emit(&app, "✓ Continuity check saved to database.");

    GenreResult { success: true, report: content, error: String::new(), run_ts }
}

// ── Series-scope command ────────────────────────────────────────────────────

pub async fn check_continuity_for_series(app: AppCtx, request: SeriesContinuityRequest) -> GenreResult {
    if let Err(msg) = crate::ai::ai_ready(&request.provider, &request.api_key, &request.model) {
        return err(&msg);
    }
    crate::reset_cancel();
    let database = app.db.as_ref();

    let books_meta = db::list_series_books(&database.pool, request.series_id).await;
    let books_meta = match books_meta {
        Ok(b) if !b.is_empty() => b,
        Ok(_) => return err("This series has no books yet. Add stories to it first."),
        Err(e) => return err(&e),
    };

    // For series bible: try explicit path first, then discover from first book's documents
    let bible = if !request.bible_path.is_empty() {
        crate::prompts::load_bible(&request.bible_path)
    } else {
        let first_story = &books_meta[0].story_id;
        crate::prompts::load_bible_tiered(&app.db, first_story, "", BibleTier::Medium).await
    };
    let extraction_model = crate::ai::resolve_slot_model(&request.extraction_model, &request.model)
        .unwrap_or_else(|_| request.model.clone());

    emit(&app, &format!("Series has {} book(s) in reading order.", books_meta.len()));

    let mut books: Vec<Book> = Vec::new();
    for meta in &books_meta {
        emit(&app, &format!("— {} —", meta.story_name));
        let book = match extract_book_facts(&app, &database, &meta.story_id, &meta.story_name, &request.provider, &request.api_key, &extraction_model, &bible).await {
            Ok(b) => b,
            Err(e) => { emit(&app, &format!("  ⚠ Skipping {}: {}", meta.story_name, e)); continue; }
        };
        let _ = db::replace_continuity_facts(&database.pool, &meta.story_id, &flatten_facts(&book)).await;
        books.push(book);
        if crate::is_cancelled() { emit(&app, "⚠ Cancelled."); return err("Cancelled."); }
    }

    if books.is_empty() { return err("Could not extract facts from any book in this series."); }

    emit(&app, "Comparing facts across the whole series for contradictions...");
    let findings = match judge_contradictions(&app, &database, &request.provider, &request.api_key, &request.model, &books, &bible).await {
        Ok(f) => f,
        Err(e) => return err(&e),
    };
    emit(&app, &format!("  {} finding(s) worth reviewing.", findings.len()));

    let scope_key = format!("series:{}", request.series_id);
    let run_ts = chrono::Utc::now().to_rfc3339();
    let _ = db::replace_continuity_findings(&database.pool, "series", &scope_key, &findings).await;

    let content = render_findings_json(&findings, "series", &scope_key);
    let _ = db::save_document_at(&database.pool, &scope_key, "continuity_check", &content, &run_ts).await;
    emit(&app, "✓ Series continuity check saved to database.");

    GenreResult { success: true, report: content, error: String::new(), run_ts }
}

// ── Shared: extract facts for every chapter in one book ────────────────────

async fn extract_book_facts(
    app: &AppCtx,
    db: &crate::db::Db,
    story_id: &str,
    story_name: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    bible: &str,
) -> Result<Book, String> {
    if !crate::stories::story_exists(db, story_id).await {
        return Err("Story not found.".to_string());
    }

    let chapters_docs = documents::list_chapters_db(db, story_id).await?;
    if chapters_docs.is_empty() {
        return Err("No chapter documents found.".to_string());
    }

    emit(app, &format!("  Extracting continuity facts from {} chapter(s)...", chapters_docs.len()));

    let mut chapter_meta: Vec<(usize, String, String)> = Vec::new();
    let mut batch_items: Vec<CachedBatchItem> = Vec::new();

    for (i, chapter) in chapters_docs.iter().enumerate() {
        let fname = documents::chapter_display_name(chapter);
        let raw = chapter.content.trim();
        if raw.is_empty() { continue; }
        let title = if !chapter.title.is_empty() {
            chapter.title.clone()
        } else {
            extract_title(raw).unwrap_or_else(|| fname.clone())
        };

        let cleaned = manuscript_fingerprint::clean_for_ai(raw);
        chapter_meta.push((i, fname.clone(), title.clone()));
        batch_items.push(CachedBatchItem {
            item: BatchChapterItem {
                file: fname,
                title,
                text: truncate_words(raw, CONTINUITY_EXCERPT_WORD_LIMIT),
            },
            source_hash: manuscript_fingerprint::chapter_source_hash(&cleaned),
        });
    }

    let results = batch_prompt::process_chapters_batched(
        app,
        db,
        provider,
        api_key,
        model,
        "continuity_extract_batch",
        "continuity_extract",
        bible,
        story_id,
        Some("continuity_extract"),
        batch_items,
        DEFAULT_WORD_BUDGET,
        &[],
    )
    .await;

    let mut chapters = Vec::with_capacity(chapter_meta.len());
    for (i, fname, title) in chapter_meta {
        let facts = match results.get(&fname) {
            Some(value) => parse_facts(value),
            None => Vec::new(),
        };
        emit(app, &format!("    [{}/{}] {} — {} fact(s)", i + 1, chapters_docs.len(), fname, facts.len()));
        chapters.push(ChapterFacts { chapter_index: i, file: fname, title, facts });
        if crate::is_cancelled() { break; }
    }

    Ok(Book { story_id: story_id.to_string(), story_name: story_name.to_string(), chapters })
}

fn parse_facts(value: &serde_json::Value) -> Vec<AiFact> {
    batch_prompt::chapter_array_field(value, "facts")
        .into_iter()
        .filter_map(|item| serde_json::from_value::<AiFact>(item).ok())
        .filter(|f| !f.entity.is_empty() && !f.attribute.is_empty() && !f.value.is_empty())
        .collect()
}

fn flatten_facts(book: &Book) -> Vec<db::ContinuityFactRow> {
    let mut out = Vec::new();
    for ch in &book.chapters {
        for f in &ch.facts {
            out.push(db::ContinuityFactRow {
                chapter_index: ch.chapter_index as i64,
                file:          ch.file.clone(),
                chapter_title: ch.title.clone(),
                entity:        f.entity.clone(),
                entity_type:   f.entity_type.clone(),
                attribute:     f.attribute.clone(),
                value:         f.value.clone(),
                snippet:       f.snippet.clone(),
            });
        }
    }
    out
}

// ── Entity name clustering (no AI — plain heuristic coreference) ───────────

async fn load_honorifics(db: &crate::db::Db) -> Vec<String> {
    let list = crate::db::load_lookup_string_list(&db.pool, "continuity.honorifics").await;
    if list.is_empty() { default_honorifics() } else { list }
}

fn default_honorifics() -> Vec<String> {
    [
        "mr", "mrs", "ms", "mx", "dr", "detective", "captain", "officer", "professor",
        "father", "sister", "aunt", "uncle", "sir", "lady", "sgt", "lieutenant", "lt", "reverend",
    ].into_iter().map(String::from).collect()
}

fn normalize_entity_name(raw: &str, honorifics: &[String]) -> String {
    let lower = raw.trim().to_lowercase();
    let mut words: Vec<&str> = lower.split_whitespace().collect();
    while let Some(first) = words.first() {
        let stripped = first.trim_end_matches('.');
        if honorifics.iter().any(|h| h.eq_ignore_ascii_case(stripped)) {
            words.remove(0);
        } else {
            break;
        }
    }
    words.join(" ")
}

fn normalize_attribute(raw: &str) -> String {
    raw.trim().to_lowercase().replace([' ', '-'], "_")
}

/// Cluster entity names that are token-subsets of one another (e.g. "sarah"
/// and "sarah chen") under the most specific form seen, longest name first.
/// This is a heuristic approximation of coreference resolution, not true
/// resolution — it will occasionally over- or under-merge on ambiguous names.
fn cluster_entity_names(names: &[String]) -> HashMap<String, String> {
    let mut uniq: Vec<String> = {
        let set: std::collections::HashSet<String> = names.iter().cloned().collect();
        set.into_iter().collect()
    };
    uniq.sort_by(|a, b| {
        let at = a.split_whitespace().count();
        let bt = b.split_whitespace().count();
        bt.cmp(&at).then(b.len().cmp(&a.len()))
    });

    let mut canonical: Vec<String> = Vec::new();
    let mut map: HashMap<String, String> = HashMap::new();

    for name in uniq {
        if name.is_empty() { continue; }
        let tokens: std::collections::HashSet<&str> = name.split_whitespace().collect();
        let mut matched: Option<String> = None;
        for c in &canonical {
            let ctoks: std::collections::HashSet<&str> = c.split_whitespace().collect();
            if tokens.is_subset(&ctoks) || ctoks.is_subset(&tokens) {
                matched = Some(c.clone());
                break;
            }
        }
        match matched {
            Some(c) => { map.insert(name, c); }
            None => { canonical.push(name.clone()); map.insert(name.clone(), name); }
        }
    }
    map
}

// ── Candidate grouping + AI judgment ────────────────────────────────────────

struct CandidateGroup {
    entity:      String,
    attribute:   String,
    occurrences: Vec<db::ContinuityOccurrence>,
}

#[derive(serde::Deserialize)]
struct AiVerdict {
    id:          usize,
    verdict:     String,
    confidence:  i64,
    explanation: String,
}

async fn judge_contradictions(
    app: &AppCtx,
    database: &crate::db::Db,
    provider: &str,
    api_key: &str,
    model: &str,
    books: &[Book],
    bible: &str,
) -> Result<Vec<db::ContinuityFindingRow>, String> {
    let is_series = books.len() > 1;
    let honorifics = load_honorifics(database).await;

    // Flatten every fact across every book into occurrences, tagged with
    // display-friendly source info, keyed by (raw entity text, raw attribute).
    struct RawOcc {
        norm_entity: String,
        attribute:   String,
        occ:         db::ContinuityOccurrence,
    }

    let mut raw: Vec<RawOcc> = Vec::new();
    let mut all_entity_names: Vec<String> = Vec::new();

    for book in books {
        for ch in &book.chapters {
            for f in &ch.facts {
                let norm = normalize_entity_name(&f.entity, &honorifics);
                if norm.is_empty() { continue; }
                all_entity_names.push(norm.clone());
                raw.push(RawOcc {
                    norm_entity: norm,
                    attribute:   normalize_attribute(&f.attribute),
                    occ: db::ContinuityOccurrence {
                        story_id:  book.story_id.clone(),
                        story_name:    book.story_name.clone(),
                        file:          ch.file.clone(),
                        chapter_title: ch.title.clone(),
                        chapter_index: ch.chapter_index as i64,
                        value:         f.value.clone(),
                        snippet:       f.snippet.clone(),
                    },
                });
            }
        }
    }

    if raw.is_empty() { return Ok(Vec::new()); }

    let cluster_map = cluster_entity_names(&all_entity_names);

    // Group by (canonical entity, attribute).
    let mut groups: HashMap<(String, String), Vec<db::ContinuityOccurrence>> = HashMap::new();
    for r in raw {
        let canonical = cluster_map.get(&r.norm_entity).cloned().unwrap_or(r.norm_entity);
        groups.entry((canonical, r.attribute)).or_default().push(r.occ);
    }

    // Pure code pre-filter: only groups where the recorded values actually
    // differ (case/whitespace-insensitive) are worth spending AI tokens on —
    // identical repeats can never be a contradiction.
    let candidates: Vec<CandidateGroup> = groups.into_iter()
        .filter_map(|((entity, attribute), occs)| {
            let distinct: std::collections::HashSet<String> = occs.iter()
                .map(|o| o.value.trim().to_lowercase())
                .collect();
            if distinct.len() < 2 { return None; }

            // For series scope: only flag if the conflicting values come from
            // different books. Within-book contradictions are handled by the
            // single-book checker — the series checker only cares about things
            // that changed between books.
            if is_series {
                let distinct_books: std::collections::HashSet<&str> = occs.iter()
                    .map(|o| o.story_id.as_str())
                    .collect();
                if distinct_books.len() < 2 { return None; }
            }

            Some(CandidateGroup { entity, attribute, occurrences: occs })
        })
        .collect();

    emit(app, &format!("  {} entity/attribute group(s) have conflicting recorded values \u{2014} judging with AI...", candidates.len()));
    if candidates.is_empty() { return Ok(Vec::new()); }

    let mut findings = Vec::new();
    const CHUNK: usize = 15;
    let story_id_for_usage = books.first().map(|b| b.story_id.as_str());
    for (chunk_idx, chunk) in candidates.chunks(CHUNK).enumerate() {
        emit(app, &format!("  Judging batch {}/{}...", chunk_idx + 1, candidates.len().div_ceil(CHUNK)));
        match judge_batch(app, story_id_for_usage, provider, api_key, model, chunk, bible).await {
            Ok(verdicts) => {
                for v in verdicts {
                    if v.id >= chunk.len() { continue; }
                    let group = &chunk[v.id];
                    findings.push(db::ContinuityFindingRow {
                        entity:      title_case(&group.entity),
                        attribute:   group.attribute.clone(),
                        verdict:     v.verdict,
                        confidence:  v.confidence.clamp(0, 100),
                        explanation: v.explanation,
                        occurrences: group.occurrences.clone(),
                    });
                }
            }
            Err(e) => emit(app, &format!("    \u{26a0} Batch judging failed: {}", e)),
        }
        if crate::is_cancelled() { break; }
    }

    findings.sort_by(|a, b| {
        let rank = |v: &str| match v { "contradiction" => 0, "possible" => 1, _ => 2 };
        rank(&a.verdict).cmp(&rank(&b.verdict)).then(b.confidence.cmp(&a.confidence))
    });

    Ok(findings)
}

async fn judge_batch(
    app: &AppCtx,
    story_id: Option<&str>,
    provider: &str,
    api_key: &str,
    model: &str,
    groups: &[CandidateGroup],
    bible: &str,
) -> Result<Vec<AiVerdict>, String> {
    let payload: Vec<serde_json::Value> = groups.iter().enumerate().map(|(i, g)| {
        serde_json::json!({
            "id": i,
            "entity": g.entity,
            "attribute": g.attribute,
            "occurrences": g.occurrences.iter().map(|o| serde_json::json!({
                "book": o.story_name,
                "chapter": o.chapter_title,
                "value": o.value,
                "snippet": o.snippet,
            })).collect::<Vec<_>>(),
        })
    }).collect();

    let candidates_json = serde_json::to_string(&payload).unwrap_or_default();

    let mut vars = std::collections::HashMap::new();
    vars.insert("bible", bible);
    vars.insert("candidates", candidates_json.as_str());

    let raw = crate::prompts::execute_prompt(
        app,
        "continuity_judge",
        provider,
        api_key,
        model,
        vars,
        story_id,
    )
    .await?;
    let clean = raw.trim()
        .trim_start_matches("```json").trim_start_matches("```")
        .trim_end_matches("```").trim();

    serde_json::from_str::<Vec<AiVerdict>>(clean)
        .map_err(|e| format!("Parse error (verdicts): {} | got: {}", e, &clean[..clean.len().min(200)]))
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ── Rendering ────────────────────────────────────────────────────────────────

fn render_findings_json(findings: &[db::ContinuityFindingRow], scope: &str, scope_key: &str) -> String {
    let contradictions = findings.iter().filter(|f| f.verdict == "contradiction").count();
    let possible = findings.iter().filter(|f| f.verdict == "possible").count();

    let json = serde_json::json!({
        "schema": "continuity_v1",
        "scope": scope,
        "scope_key": scope_key,
        "note": "AI-assisted: facts are extracted per chapter, then compared for contradictions. Extraction and judgment can both make mistakes — treat this as a prompt to go check the source text, not a verdict.",
        "summary": {
            "total_findings": findings.len(),
            "contradictions": contradictions,
            "possible": possible,
            "likely_intentional": findings.len().saturating_sub(contradictions).saturating_sub(possible),
        },
        "findings": findings.iter().map(|f| serde_json::json!({
            "entity": f.entity,
            "attribute": f.attribute,
            "verdict": f.verdict,
            "confidence": f.confidence,
            "explanation": f.explanation,
            "occurrences": f.occurrences.iter().map(|o| serde_json::json!({
                "story_name": o.story_name,
                "file": o.file,
                "chapter_title": o.chapter_title,
                "chapter_index": o.chapter_index,
                "value": o.value,
                "snippet": o.snippet,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    json.to_string()
}

// ── Suggest fix for a continuity finding ─────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct SuggestFixRequest {
    pub provider:   String,
    pub api_key:    String,
    pub model:      String,
    pub entity:     String,
    pub attribute:  String,
    pub explanation: String,
    pub occurrences: Vec<SuggestFixOccurrence>,
    #[serde(default)]
    #[serde(alias = "folder")]
    pub story_id:     String,
    #[serde(default)]
    pub bible_path: String,
}

#[derive(serde::Deserialize)]
pub struct SuggestFixOccurrence {
    pub story_name:    String,
    pub file:          String,
    pub chapter_title: String,
    pub value:         String,
    pub snippet:       String,
}

#[derive(serde::Serialize)]
pub struct SuggestFixResult {
    pub success: bool,
    pub suggestions: String,
    pub error: String,
}

pub async fn suggest_continuity_fix(app: AppCtx, request: SuggestFixRequest) -> SuggestFixResult {
    use std::collections::HashMap;

    let database = app.db.as_ref();
    let bible = crate::prompts::load_bible_for_story(&app.db, &request.story_id, &request.bible_path).await;

    let mut occurrences_text = String::new();
    for occ in &request.occurrences {
        occurrences_text.push_str(&format!(
            "\n- {} / {} ({}): value = \"{}\"\n  Context: \"{}\"",
            occ.story_name, occ.chapter_title, occ.file, occ.value, occ.snippet
        ));
    }

    let mut vars = HashMap::new();
    vars.insert("entity", request.entity.as_str());
    vars.insert("attribute", request.attribute.as_str());
    vars.insert("explanation", request.explanation.as_str());
    vars.insert("occurrences", occurrences_text.as_str());
    vars.insert("bible", bible.as_str());

    match crate::prompts::execute_prompt(
        &app,
        "continuity_suggest",
        &request.provider,
        &request.api_key,
        &request.model,
        vars,
        None,
    )
    .await {
        Ok(suggestions) => SuggestFixResult { success: true, suggestions, error: String::new() },
        Err(e) => SuggestFixResult { success: false, suggestions: String::new(), error: e },
    }
}
