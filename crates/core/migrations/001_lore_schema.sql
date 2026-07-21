CREATE SCHEMA IF NOT EXISTS lore;

SET search_path TO lore;

CREATE TABLE genres (
    id          BIGSERIAL PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    description TEXT
);

CREATE TABLE kdp_categories (
    id             BIGSERIAL PRIMARY KEY,
    path           TEXT NOT NULL,
    store          TEXT NOT NULL DEFAULT 'Kindle',
    amazon_node_id TEXT,
    source         TEXT NOT NULL DEFAULT 'manual',
    verified_at    TEXT,
    created_at     TEXT NOT NULL,
    last_seen_at   TEXT,
    UNIQUE(path, store)
);

CREATE TABLE genre_kdp_links (
    genre_id    BIGINT NOT NULL REFERENCES genres(id) ON DELETE CASCADE,
    category_id BIGINT NOT NULL REFERENCES kdp_categories(id) ON DELETE CASCADE,
    PRIMARY KEY (genre_id, category_id)
);

CREATE TABLE genre_rankings (
    id           BIGSERIAL PRIMARY KEY,
    story_id     TEXT NOT NULL,
    genre_id     BIGINT NOT NULL REFERENCES genres(id),
    confidence   INTEGER NOT NULL,
    reason       TEXT,
    generated_at TEXT NOT NULL
);

CREATE TABLE category_results (
    id            BIGSERIAL PRIMARY KEY,
    story_id      TEXT NOT NULL,
    category_id   BIGINT REFERENCES kdp_categories(id),
    raw_path      TEXT NOT NULL,
    store         TEXT NOT NULL,
    confidence    INTEGER NOT NULL,
    sales_to_one  TEXT,
    sales_to_ten  TEXT,
    publisher_pct TEXT,
    ku_pct        TEXT,
    status        TEXT NOT NULL,
    note          TEXT,
    generated_at  TEXT NOT NULL
);

CREATE INDEX idx_rankings_folder ON genre_rankings(story_id);
CREATE INDEX idx_results_folder ON category_results(story_id);
CREATE INDEX idx_categories_path ON kdp_categories(path);
CREATE INDEX idx_categories_store ON kdp_categories(store);

CREATE TABLE chapter_summaries (
    id           BIGSERIAL PRIMARY KEY,
    story_id     TEXT NOT NULL,
    file         TEXT NOT NULL,
    title        TEXT,
    signals      TEXT,
    word_count   INTEGER,
    updated_at   TEXT NOT NULL,
    UNIQUE(story_id, file)
);

CREATE TABLE genre_data (
    story_id             TEXT PRIMARY KEY,
    generated_at         TEXT NOT NULL,
    industry_ebook       TEXT,
    industry_print       TEXT,
    genre_signals        TEXT,
    reader_demographic   TEXT,
    bookstore_shelving   TEXT,
    kdp_ebook_json       TEXT NOT NULL DEFAULT '[]',
    kdp_print_json       TEXT NOT NULL DEFAULT '[]',
    comps_ebook_json     TEXT NOT NULL DEFAULT '[]',
    comps_print_json     TEXT NOT NULL DEFAULT '[]',
    marketing_notes_json TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE kdp_keywords (
    story_id      TEXT PRIMARY KEY,
    generated_at  TEXT NOT NULL,
    keywords_json TEXT NOT NULL,
    strategy      TEXT,
    source_note   TEXT
);

CREATE TABLE mi_search_terms (
    story_id      TEXT PRIMARY KEY,
    generated_at  TEXT NOT NULL,
    keywords_json TEXT NOT NULL
);

CREATE TABLE discovery_keywords (
    story_id      TEXT PRIMARY KEY,
    generated_at  TEXT NOT NULL,
    keywords_json TEXT NOT NULL
);

CREATE TABLE keyword_search_results (
    id           BIGSERIAL PRIMARY KEY,
    story_id     TEXT NOT NULL,
    seed         TEXT NOT NULL,
    keyword      TEXT NOT NULL,
    searches     TEXT,
    competition  TEXT,
    earnings     TEXT,
    generated_at TEXT NOT NULL
);

CREATE INDEX idx_keyword_results_folder ON keyword_search_results(story_id);

CREATE TABLE story_documents (
    id           BIGSERIAL PRIMARY KEY,
    story_id     TEXT NOT NULL,
    doc_type     TEXT NOT NULL,
    content      TEXT NOT NULL,
    generated_at TEXT NOT NULL
);

CREATE INDEX idx_story_docs_folder ON story_documents(story_id, doc_type);
CREATE INDEX idx_summaries_folder ON chapter_summaries(story_id);

CREATE TABLE bisac_codes (
    code    TEXT PRIMARY KEY,
    heading TEXT NOT NULL
);

CREATE TABLE bisac_classifications (
    id           BIGSERIAL PRIMARY KEY,
    story_id     TEXT NOT NULL,
    code         TEXT NOT NULL,
    heading      TEXT NOT NULL,
    confidence   INTEGER NOT NULL,
    reason       TEXT,
    generated_at TEXT NOT NULL,
    format       TEXT NOT NULL DEFAULT 'ebook'
);

CREATE INDEX idx_bisac_folder ON bisac_classifications(story_id);

CREATE TABLE saved_reports (
    id       BIGSERIAL PRIMARY KEY,
    story_id TEXT NOT NULL,
    doc_type TEXT NOT NULL,
    version  INTEGER NOT NULL,
    label    TEXT NOT NULL,
    content  TEXT NOT NULL,
    saved_at TEXT NOT NULL
);

CREATE INDEX idx_saved_reports_folder ON saved_reports(story_id, doc_type);

CREATE TABLE report_types (
    id               TEXT PRIMARY KEY,
    label            TEXT NOT NULL,
    description      TEXT NOT NULL,
    platforms        TEXT NOT NULL DEFAULT 'kdp,wide',
    depends_on       TEXT NOT NULL DEFAULT '',
    cost_truncation  INTEGER NOT NULL DEFAULT 4000,
    cost_output_max  INTEGER NOT NULL DEFAULT 1000,
    cost_per_chapter INTEGER NOT NULL DEFAULT 0,
    cost_fixed_calls INTEGER NOT NULL DEFAULT 1,
    model_slot       TEXT NOT NULL DEFAULT 'default',
    min_tier         TEXT NOT NULL DEFAULT 'basic'
);

CREATE TABLE provider_models (
    id           TEXT PRIMARY KEY,
    provider     TEXT NOT NULL,
    owned_by     TEXT NOT NULL DEFAULT '',
    input_price  DOUBLE PRECISION,
    output_price DOUBLE PRECISION,
    sort_order   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE lookup_config (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE zeigarnik_config (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE zeigarnik_chapters (
    id             BIGSERIAL PRIMARY KEY,
    story_id       TEXT NOT NULL,
    chapter_index  INTEGER NOT NULL,
    file           TEXT NOT NULL,
    title          TEXT NOT NULL,
    word_count     INTEGER NOT NULL,
    sentence_count INTEGER NOT NULL,
    question_count INTEGER NOT NULL,
    ending_type    TEXT NOT NULL,
    tension_score  INTEGER NOT NULL,
    ending_snippet TEXT NOT NULL,
    generated_at   TEXT NOT NULL
);

CREATE INDEX idx_zeigarnik_chapters_folder ON zeigarnik_chapters(story_id);

CREATE TABLE zeigarnik_threads (
    id                  BIGSERIAL PRIMARY KEY,
    story_id            TEXT NOT NULL,
    term                TEXT NOT NULL,
    mention_count       INTEGER NOT NULL,
    first_chapter_index INTEGER NOT NULL,
    first_file          TEXT NOT NULL,
    first_snippet       TEXT NOT NULL,
    gap_start_index     INTEGER NOT NULL,
    gap_end_index       INTEGER NOT NULL,
    max_gap_chapters    INTEGER NOT NULL,
    max_gap_words       INTEGER NOT NULL,
    generated_at        TEXT NOT NULL
);

CREATE INDEX idx_zeigarnik_threads_folder ON zeigarnik_threads(story_id);

CREATE TABLE "series" (
    id         BIGSERIAL PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    bible_path TEXT NOT NULL DEFAULT ''
);

CREATE TABLE series_books (
    series_id  BIGINT NOT NULL REFERENCES "series"(id) ON DELETE CASCADE,
    story_id   TEXT NOT NULL,
    story_name TEXT NOT NULL DEFAULT '',
    book_order INTEGER NOT NULL,
    PRIMARY KEY (series_id, story_id)
);

CREATE INDEX idx_series_books_series ON series_books(series_id);

CREATE TABLE continuity_facts (
    id            BIGSERIAL PRIMARY KEY,
    story_id      TEXT NOT NULL,
    chapter_index INTEGER NOT NULL,
    file          TEXT NOT NULL,
    chapter_title TEXT NOT NULL,
    entity        TEXT NOT NULL,
    entity_type   TEXT NOT NULL,
    attribute     TEXT NOT NULL,
    value         TEXT NOT NULL,
    snippet       TEXT NOT NULL,
    generated_at  TEXT NOT NULL
);

CREATE INDEX idx_continuity_facts_folder ON continuity_facts(story_id);

CREATE TABLE continuity_findings (
    id               BIGSERIAL PRIMARY KEY,
    scope            TEXT NOT NULL,
    scope_key        TEXT NOT NULL,
    entity           TEXT NOT NULL,
    attribute        TEXT NOT NULL,
    verdict          TEXT NOT NULL,
    confidence       INTEGER NOT NULL,
    explanation      TEXT NOT NULL,
    occurrences_json TEXT NOT NULL,
    generated_at     TEXT NOT NULL
);

CREATE INDEX idx_continuity_findings_scope ON continuity_findings(scope, scope_key);

CREATE TABLE prompt_templates (
    id            TEXT PRIMARY KEY,
    label         TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    user_template TEXT NOT NULL,
    max_tokens    INTEGER NOT NULL DEFAULT 4000,
    json_mode     INTEGER NOT NULL DEFAULT 0,
    version       INTEGER NOT NULL DEFAULT 1,
    updated_at    TEXT NOT NULL DEFAULT ''
);

CREATE TABLE preprocessed_chapters (
    id                 BIGSERIAL PRIMARY KEY,
    story_id           TEXT NOT NULL,
    chapter_file       TEXT NOT NULL,
    report_type        TEXT NOT NULL,
    processed_text     TEXT NOT NULL,
    source_modified_at TEXT NOT NULL,
    created_at         TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_preproc_unique ON preprocessed_chapters(story_id, chapter_file, report_type);

CREATE TABLE stories (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    created    TEXT NOT NULL,
    bible_path TEXT NOT NULL DEFAULT ''
);

CREATE TABLE manuscripts (
    id         BIGSERIAL PRIMARY KEY,
    story_id   TEXT NOT NULL,
    kind       TEXT NOT NULL DEFAULT 'chapter',
    title      TEXT NOT NULL DEFAULT '',
    path_hint  TEXT NOT NULL DEFAULT '',
    content    TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_manuscripts_story ON manuscripts(story_id, kind);
CREATE INDEX idx_manuscripts_path ON manuscripts(story_id, path_hint);
