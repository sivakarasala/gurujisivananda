CREATE TABLE audio_tracks (
    id               UUID        NOT NULL PRIMARY KEY,
    youtube_id       TEXT        NOT NULL UNIQUE,
    title            TEXT        NOT NULL,
    channel          TEXT        NOT NULL,
    duration_seconds INTEGER     NOT NULL DEFAULT 0,
    upload_date      DATE,
    description      TEXT        NOT NULL DEFAULT '',
    tags             TEXT[]      NOT NULL DEFAULT '{}',
    thumbnail_url    TEXT        NOT NULL DEFAULT '',
    file_path        TEXT        NOT NULL,
    file_size        BIGINT      NOT NULL DEFAULT 0,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audio_tracks_title_fts ON audio_tracks USING gin (to_tsvector('english', title));
CREATE INDEX idx_audio_tracks_channel ON audio_tracks (channel);
