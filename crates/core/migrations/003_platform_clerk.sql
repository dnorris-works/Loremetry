-- Clerk sign-in (public values; configured in Admin, not env vars).
ALTER TABLE lore.platform_secrets
    ADD COLUMN IF NOT EXISTS clerk_publishable_key TEXT NOT NULL DEFAULT '';
ALTER TABLE lore.platform_secrets
    ADD COLUMN IF NOT EXISTS clerk_jwt_issuer TEXT NOT NULL DEFAULT '';
