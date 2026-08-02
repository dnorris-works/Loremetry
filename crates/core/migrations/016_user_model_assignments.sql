ALTER TABLE lore.users
    ADD COLUMN IF NOT EXISTS model_assignments JSONB NOT NULL DEFAULT '{}';
