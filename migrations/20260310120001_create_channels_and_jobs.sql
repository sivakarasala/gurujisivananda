CREATE TABLE channels (
    id             UUID        NOT NULL PRIMARY KEY,
    name           TEXT        NOT NULL,
    youtube_url    TEXT        NOT NULL UNIQUE,
    auto_sync      BOOLEAN     NOT NULL DEFAULT true,
    last_synced_at TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE download_jobs (
    id              UUID        NOT NULL PRIMARY KEY,
    url             TEXT        NOT NULL,
    url_type        TEXT        NOT NULL,
    channel_id      UUID        REFERENCES channels(id) ON DELETE SET NULL,
    status          TEXT        NOT NULL DEFAULT 'pending',
    created_by      UUID        REFERENCES users(id),
    error_message   TEXT,
    tracks_found    INTEGER     NOT NULL DEFAULT 0,
    tracks_imported INTEGER     NOT NULL DEFAULT 0,
    tracks_skipped  INTEGER     NOT NULL DEFAULT 0,
    tracks_errored  INTEGER     NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_download_jobs_status ON download_jobs (status);
CREATE INDEX idx_download_jobs_created_at ON download_jobs (created_at DESC);
