-- Archive superseded report snapshots (Settings → Archived Reports).

ALTER TABLE lore.story_documents
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'current';

ALTER TABLE lore.story_documents
    ADD COLUMN IF NOT EXISTS archived_at TEXT;

ALTER TABLE lore.story_documents
    ADD COLUMN IF NOT EXISTS archive_reason TEXT;

UPDATE lore.story_documents
SET status = 'current'
WHERE status IS NULL OR status = '';
