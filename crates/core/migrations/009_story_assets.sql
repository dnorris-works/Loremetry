-- story_assets: slot-based author content (replaces lore.manuscripts for new writes).

CREATE TABLE lore.story_assets (
    id            BIGSERIAL PRIMARY KEY,
    story_id      TEXT NOT NULL,
    slot          TEXT NOT NULL,
    title         TEXT NOT NULL DEFAULT '',
    filename      TEXT NOT NULL,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    content       TEXT NOT NULL DEFAULT '',
    content_hash  TEXT NOT NULL DEFAULT '',
    source_format TEXT NOT NULL DEFAULT 'md',
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_story_assets_key
    ON lore.story_assets (story_id, slot, lower(filename));

CREATE INDEX idx_story_assets_story_slot
    ON lore.story_assets (story_id, slot, sort_order, filename);

-- Migrate existing manuscripts rows into story_assets.
INSERT INTO lore.story_assets (
    story_id, slot, title, filename, sort_order, content, content_hash, source_format, created_at, updated_at
)
SELECT
    m.story_id,
    CASE m.kind
        WHEN 'chapter' THEN 'manuscript'
        WHEN 'bible' THEN 'bible'
        WHEN 'character' THEN 'character'
        WHEN 'location' THEN 'location'
        ELSE 'manuscript'
    END AS slot,
    m.title,
    CASE
        WHEN m.path_hint <> '' AND position('/' IN m.path_hint) > 0
            THEN regexp_replace(m.path_hint, '^.*/', '')
        WHEN m.path_hint <> '' THEN m.path_hint
        WHEN m.title <> '' THEN m.title || '.md'
        ELSE 'untitled-' || m.id::text || '.md'
    END AS filename,
    m.id::integer AS sort_order,
    m.content,
    '' AS content_hash,
    'md' AS source_format,
    m.updated_at AS created_at,
    m.updated_at
FROM lore.manuscripts m;
