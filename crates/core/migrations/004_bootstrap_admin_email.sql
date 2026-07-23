-- Bootstrap admin identity (open mode / pre-Clerk); configured in Admin, not deploy env.
ALTER TABLE lore.platform_secrets
    ADD COLUMN IF NOT EXISTS bootstrap_admin_email TEXT NOT NULL DEFAULT 'admin@local';
