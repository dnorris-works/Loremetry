ALTER TABLE lore.users
    ADD COLUMN IF NOT EXISTS theme_preference TEXT NOT NULL DEFAULT 'light'
    CHECK (theme_preference IN ('light', 'dark'));
