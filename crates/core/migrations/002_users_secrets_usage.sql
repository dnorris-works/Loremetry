-- Idempotent: safe if a prior deploy failed mid-migration.
CREATE TABLE IF NOT EXISTS lore.users (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    clerk_id            TEXT UNIQUE,
    email               TEXT NOT NULL UNIQUE,
    role                TEXT NOT NULL DEFAULT 'subscriber'
                        CHECK (role IN ('admin', 'subscriber')),
    plan_label          TEXT NOT NULL DEFAULT '',
    monthly_fee_cents   INTEGER NOT NULL DEFAULT 0,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS lore.platform_secrets (
    id                  SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    anthropic_api_key   BYTEA NOT NULL DEFAULT ''::bytea,
    tokenmix_api_key    BYTEA NOT NULL DEFAULT ''::bytea,
    canopy_api_key      BYTEA NOT NULL DEFAULT ''::bytea,
    dataforseo_login    BYTEA NOT NULL DEFAULT ''::bytea,
    dataforseo_password BYTEA NOT NULL DEFAULT ''::bytea,
    default_provider    TEXT NOT NULL DEFAULT 'claude',
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO lore.platform_secrets (id) VALUES (1)
ON CONFLICT (id) DO NOTHING;

CREATE TABLE IF NOT EXISTS lore.ai_usage_events (
    id              BIGSERIAL PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES lore.users(id) ON DELETE CASCADE,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    kind            TEXT NOT NULL CHECK (kind IN ('llm', 'canopy', 'dataforseo')),
    provider        TEXT NOT NULL DEFAULT '',
    model           TEXT NOT NULL DEFAULT '',
    feature         TEXT NOT NULL DEFAULT '',
    story_id        TEXT,
    input_tokens    INTEGER NOT NULL DEFAULT 0,
    output_tokens   INTEGER NOT NULL DEFAULT 0,
    cost_usd        DOUBLE PRECISION NOT NULL DEFAULT 0,
    metadata        JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS idx_ai_usage_user_time ON lore.ai_usage_events(user_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_ai_usage_occurred ON lore.ai_usage_events(occurred_at);

ALTER TABLE lore.stories
    ADD COLUMN IF NOT EXISTS owner_user_id UUID REFERENCES lore.users(id) ON DELETE SET NULL;
