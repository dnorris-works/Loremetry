-- Background job queue for long-running analysis (worker process).

CREATE TABLE lore.jobs (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_type         TEXT NOT NULL,
    story_id         TEXT NOT NULL,
    user_id          UUID NOT NULL,
    status           TEXT NOT NULL DEFAULT 'pending',
    payload          JSONB NOT NULL,
    result           JSONB,
    error            TEXT,
    cancel_requested BOOLEAN NOT NULL DEFAULT false,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at       TIMESTAMPTZ,
    finished_at      TIMESTAMPTZ
);

CREATE INDEX idx_jobs_pending ON lore.jobs (status, created_at)
    WHERE status = 'pending';

CREATE INDEX idx_jobs_story_user ON lore.jobs (story_id, user_id);

CREATE TABLE lore.job_events (
    id         BIGSERIAL PRIMARY KEY,
    job_id     UUID NOT NULL REFERENCES lore.jobs(id) ON DELETE CASCADE,
    channel    TEXT NOT NULL,
    message    TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_job_events_job_id ON lore.job_events (job_id, id);
