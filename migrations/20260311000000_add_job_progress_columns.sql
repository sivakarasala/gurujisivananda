ALTER TABLE download_jobs
    ADD COLUMN download_current_item INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN download_total_items  INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN download_percent      REAL    NOT NULL DEFAULT 0.0,
    ADD COLUMN pid                   INTEGER;
