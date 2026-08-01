-- Desktop parity: chapter AI result cache, Google keyword results, manuscript/artifact freshness.

CREATE TABLE IF NOT EXISTS chapter_ai_cache (
    story_id     TEXT NOT NULL,
    chapter_file TEXT NOT NULL,
    report_type  TEXT NOT NULL,
    source_hash  TEXT NOT NULL,
    result_json  TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    PRIMARY KEY (story_id, chapter_file, report_type)
);

CREATE TABLE IF NOT EXISTS google_keyword_search_results (
    id           BIGSERIAL PRIMARY KEY,
    story_id     TEXT NOT NULL,
    keyword      TEXT NOT NULL,
    searches     TEXT,
    competition  TEXT,
    cpc          TEXT,
    generated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_google_keyword_results_story
    ON google_keyword_search_results(story_id);

CREATE TABLE IF NOT EXISTS story_manuscript_state (
    story_id                TEXT PRIMARY KEY,
    manuscript_fingerprint  TEXT NOT NULL,
    updated_at              TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS story_artifact_state (
    story_id                             TEXT NOT NULL,
    artifact_type                        TEXT NOT NULL,
    built_from_manuscript_fingerprint    TEXT,
    updated_at                           TEXT,
    PRIMARY KEY (story_id, artifact_type)
);
