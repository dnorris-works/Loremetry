-- Operator break-glass token (full API without Clerk); set in Admin → Platform credentials.
ALTER TABLE lore.platform_secrets
    ADD COLUMN IF NOT EXISTS admin_bypass_token TEXT NOT NULL DEFAULT '';
