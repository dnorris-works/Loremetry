-- Report freshness: manuscript fingerprint at save + chapter summary source hashes.

ALTER TABLE lore.story_documents
    ADD COLUMN IF NOT EXISTS manuscript_fingerprint_at_save TEXT NOT NULL DEFAULT '';

ALTER TABLE lore.chapter_summaries
    ADD COLUMN IF NOT EXISTS source_hash TEXT NOT NULL DEFAULT '';
