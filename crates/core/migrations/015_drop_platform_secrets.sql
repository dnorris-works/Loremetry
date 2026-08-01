-- Drop the platform_secrets table.
-- Credentials are now loaded exclusively from environment variables at startup.

DROP TABLE IF EXISTS platform_secrets;
