-- Marketing mode: ad campaigns, creatives, platform accounts (ported from desktop).

CREATE TABLE lore.ad_platform_accounts (
    id             BIGSERIAL PRIMARY KEY,
    platform       TEXT NOT NULL,
    account_id     TEXT NOT NULL DEFAULT '',
    pixel_id       TEXT NOT NULL DEFAULT '',
    tracking_notes TEXT NOT NULL DEFAULT '',
    payment_notes  TEXT NOT NULL DEFAULT '',
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

CREATE TABLE lore.ad_landing_pages (
    id              BIGSERIAL PRIMARY KEY,
    story_id        TEXT NOT NULL,
    name            TEXT NOT NULL,
    url             TEXT NOT NULL DEFAULT '',
    conversion_rate DOUBLE PRECISION,
    notes           TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX idx_ad_landing_pages_story ON lore.ad_landing_pages(story_id);

CREATE TABLE lore.ad_campaigns (
    id                  BIGSERIAL PRIMARY KEY,
    story_id            TEXT NOT NULL,
    name                TEXT NOT NULL,
    platform            TEXT NOT NULL DEFAULT '',
    platform_account_id BIGINT REFERENCES lore.ad_platform_accounts(id),
    objective           TEXT NOT NULL DEFAULT 'awareness',
    status              TEXT NOT NULL DEFAULT 'draft',
    budget              DOUBLE PRECISION,
    budget_period       TEXT NOT NULL DEFAULT 'lifetime',
    start_date          TEXT NOT NULL DEFAULT '',
    end_date            TEXT NOT NULL DEFAULT '',
    target_audience     TEXT NOT NULL DEFAULT '',
    landing_page_id     BIGINT REFERENCES lore.ad_landing_pages(id),
    notes               TEXT NOT NULL DEFAULT '',
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE INDEX idx_ad_campaigns_story ON lore.ad_campaigns(story_id);

CREATE TABLE lore.ad_creatives (
    id              BIGSERIAL PRIMARY KEY,
    campaign_id     BIGINT NOT NULL REFERENCES lore.ad_campaigns(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    creative_type   TEXT NOT NULL DEFAULT 'video',
    version         TEXT NOT NULL DEFAULT 'v1',
    platform_format TEXT NOT NULL DEFAULT '',
    status          TEXT NOT NULL DEFAULT 'draft',
    asset_path      TEXT NOT NULL DEFAULT '',
    body_text       TEXT NOT NULL DEFAULT '',
    notes           TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX idx_ad_creatives_campaign ON lore.ad_creatives(campaign_id);

CREATE TABLE lore.ad_performance_snapshots (
    id            BIGSERIAL PRIMARY KEY,
    campaign_id   BIGINT NOT NULL REFERENCES lore.ad_campaigns(id) ON DELETE CASCADE,
    creative_id   BIGINT REFERENCES lore.ad_creatives(id) ON DELETE SET NULL,
    snapshot_date TEXT NOT NULL,
    impressions   BIGINT NOT NULL DEFAULT 0,
    clicks        BIGINT NOT NULL DEFAULT 0,
    conversions   BIGINT NOT NULL DEFAULT 0,
    ctr           DOUBLE PRECISION NOT NULL DEFAULT 0,
    cpc           DOUBLE PRECISION NOT NULL DEFAULT 0,
    cpa           DOUBLE PRECISION NOT NULL DEFAULT 0,
    spend         DOUBLE PRECISION NOT NULL DEFAULT 0,
    notes         TEXT NOT NULL DEFAULT '',
    created_at    TEXT NOT NULL
);

CREATE INDEX idx_ad_perf_campaign_date ON lore.ad_performance_snapshots(campaign_id, snapshot_date);

CREATE TABLE lore.ad_spend_entries (
    id          BIGSERIAL PRIMARY KEY,
    campaign_id BIGINT NOT NULL REFERENCES lore.ad_campaigns(id) ON DELETE CASCADE,
    platform    TEXT NOT NULL DEFAULT '',
    amount      DOUBLE PRECISION NOT NULL DEFAULT 0,
    spent_at    TEXT NOT NULL,
    notes       TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL
);

CREATE INDEX idx_ad_spend_campaign ON lore.ad_spend_entries(campaign_id);

CREATE TABLE lore.ad_audience_notes (
    id             BIGSERIAL PRIMARY KEY,
    campaign_id    BIGINT NOT NULL REFERENCES lore.ad_campaigns(id) ON DELETE CASCADE,
    label          TEXT NOT NULL DEFAULT '',
    demographics   TEXT NOT NULL DEFAULT '',
    interests      TEXT NOT NULL DEFAULT '',
    lookalike_notes TEXT NOT NULL DEFAULT '',
    outcome        TEXT NOT NULL DEFAULT '',
    notes          TEXT NOT NULL DEFAULT '',
    created_at     TEXT NOT NULL
);
