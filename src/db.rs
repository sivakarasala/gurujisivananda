#[cfg(feature = "ssr")]
pub async fn db() -> Result<sqlx::PgPool, leptos::prelude::ServerFnError> {
    use leptos::prelude::*;

    use_context::<sqlx::PgPool>()
        .ok_or_else(|| ServerFnError::new("Database pool not found in context"))
}

#[cfg(feature = "ssr")]
#[derive(sqlx::FromRow)]
pub struct TrackRow {
    pub id: uuid::Uuid,
    pub youtube_id: String,
    pub title: String,
    pub channel: String,
    pub duration_seconds: i32,
    pub thumbnail_url: String,
    pub file_path: String,
    pub file_size: i64,
}

#[cfg(feature = "ssr")]
pub async fn search_tracks(
    pool: &sqlx::PgPool,
    query: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<TrackRow>, sqlx::Error> {
    sqlx::query_as::<_, TrackRow>(
        "SELECT id, youtube_id, title, channel, duration_seconds, thumbnail_url, file_path, file_size \
         FROM audio_tracks \
         WHERE to_tsvector('english', title) @@ plainto_tsquery('english', $1) \
         ORDER BY ts_rank(to_tsvector('english', title), plainto_tsquery('english', $1)) DESC \
         LIMIT $2 OFFSET $3",
    )
    .bind(query)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

#[cfg(feature = "ssr")]
pub async fn list_tracks(
    pool: &sqlx::PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<TrackRow>, sqlx::Error> {
    sqlx::query_as::<_, TrackRow>(
        "SELECT id, youtube_id, title, channel, duration_seconds, thumbnail_url, file_path, file_size \
         FROM audio_tracks \
         ORDER BY created_at DESC \
         LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

#[cfg(feature = "ssr")]
pub async fn get_track_by_id(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
) -> Result<Option<TrackRow>, sqlx::Error> {
    sqlx::query_as::<_, TrackRow>(
        "SELECT id, youtube_id, title, channel, duration_seconds, thumbnail_url, file_path, file_size \
         FROM audio_tracks \
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

// ---- User queries ----

#[cfg(feature = "ssr")]
#[derive(sqlx::FromRow)]
pub struct UserRow {
    pub id: uuid::Uuid,
    pub email: String,
    pub password_hash: String,
    pub role: String,
}

#[cfg(feature = "ssr")]
pub async fn get_user_by_email(
    pool: &sqlx::PgPool,
    email: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(
        "SELECT id, email, password_hash, role FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
}

// ---- Session queries ----

#[cfg(feature = "ssr")]
pub async fn create_session(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
    user_id: uuid::Uuid,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(user_id)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn delete_session(pool: &sqlx::PgPool, id: uuid::Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ---- Channel queries ----

#[cfg(feature = "ssr")]
#[derive(sqlx::FromRow, Clone, Debug)]
pub struct ChannelRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub youtube_url: String,
    pub auto_sync: bool,
    pub last_synced_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub max_downloads_per_batch: Option<i32>,
}

#[cfg(feature = "ssr")]
pub async fn create_channel(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
    name: &str,
    youtube_url: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO channels (id, name, youtube_url) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(name)
        .bind(youtube_url)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn list_channels(pool: &sqlx::PgPool) -> Result<Vec<ChannelRow>, sqlx::Error> {
    sqlx::query_as::<_, ChannelRow>(
        "SELECT id, name, youtube_url, auto_sync, last_synced_at, created_at, max_downloads_per_batch \
         FROM channels ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
}

#[cfg(feature = "ssr")]
pub async fn delete_channel(pool: &sqlx::PgPool, id: uuid::Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM channels WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn update_channel_synced(pool: &sqlx::PgPool, id: uuid::Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE channels SET last_synced_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn list_channels_for_sync(pool: &sqlx::PgPool) -> Result<Vec<ChannelRow>, sqlx::Error> {
    sqlx::query_as::<_, ChannelRow>(
        "SELECT id, name, youtube_url, auto_sync, last_synced_at, created_at, max_downloads_per_batch \
         FROM channels WHERE auto_sync = true ORDER BY created_at",
    )
    .fetch_all(pool)
    .await
}

#[cfg(feature = "ssr")]
pub async fn update_channel_batch_size(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
    batch_size: Option<i32>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE channels SET max_downloads_per_batch = $2 WHERE id = $1")
        .bind(id)
        .bind(batch_size)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn get_channel_batch_size(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
) -> Result<Option<i32>, sqlx::Error> {
    let row: Option<(Option<i32>,)> =
        sqlx::query_as("SELECT max_downloads_per_batch FROM channels WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|r| r.0))
}

#[cfg(feature = "ssr")]
pub async fn count_tracks_by_channel(
    pool: &sqlx::PgPool,
) -> Result<std::collections::HashMap<uuid::Uuid, i64>, sqlx::Error> {
    let rows: Vec<(uuid::Uuid, i64)> = sqlx::query_as(
        "SELECT channel_id, SUM(tracks_imported)::BIGINT \
         FROM download_jobs \
         WHERE channel_id IS NOT NULL AND status = 'completed' \
         GROUP BY channel_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

// ---- Download job queries ----

#[cfg(feature = "ssr")]
#[derive(sqlx::FromRow, Clone, Debug)]
pub struct DownloadJobRow {
    pub id: uuid::Uuid,
    pub url: String,
    pub url_type: String,
    pub channel_id: Option<uuid::Uuid>,
    pub status: String,
    pub error_message: Option<String>,
    pub tracks_found: i32,
    pub tracks_imported: i32,
    pub tracks_skipped: i32,
    pub tracks_errored: i32,
    pub download_current_item: i32,
    pub download_total_items: i32,
    pub download_percent: f32,
    pub pid: Option<i32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(feature = "ssr")]
pub async fn create_download_job(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
    url: &str,
    url_type: &str,
    channel_id: Option<uuid::Uuid>,
    created_by: Option<uuid::Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO download_jobs (id, url, url_type, channel_id, created_by) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(url)
    .bind(url_type)
    .bind(channel_id)
    .bind(created_by)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn list_download_jobs(
    pool: &sqlx::PgPool,
    limit: i64,
) -> Result<Vec<DownloadJobRow>, sqlx::Error> {
    sqlx::query_as::<_, DownloadJobRow>(
        "SELECT id, url, url_type, channel_id, status, error_message, tracks_found, tracks_imported, \
         tracks_skipped, tracks_errored, download_current_item, download_total_items, \
         download_percent, pid, created_at, updated_at \
         FROM download_jobs ORDER BY created_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

#[cfg(feature = "ssr")]
pub async fn update_job_status(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
    status: &str,
    error_message: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE download_jobs SET status = $2, error_message = $3, updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .bind(status)
    .bind(error_message)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn update_job_progress(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
    tracks_found: i32,
    tracks_imported: i32,
    tracks_skipped: i32,
    tracks_errored: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE download_jobs SET tracks_found = $2, tracks_imported = $3, tracks_skipped = $4, tracks_errored = $5, updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .bind(tracks_found)
    .bind(tracks_imported)
    .bind(tracks_skipped)
    .bind(tracks_errored)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn update_job_download_progress(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
    current_item: i32,
    total_items: i32,
    percent: f32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE download_jobs SET download_current_item = $2, download_total_items = $3, \
         download_percent = $4, updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .bind(current_item)
    .bind(total_items)
    .bind(percent)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn update_job_pid(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
    pid: Option<i32>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE download_jobs SET pid = $2, updated_at = NOW() WHERE id = $1")
        .bind(id)
        .bind(pid)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn get_job_status(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
) -> Result<Option<(String, Option<i32>)>, sqlx::Error> {
    sqlx::query_as::<_, (String, Option<i32>)>(
        "SELECT status, pid FROM download_jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

#[cfg(feature = "ssr")]
pub async fn get_download_job(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
) -> Result<Option<DownloadJobRow>, sqlx::Error> {
    sqlx::query_as::<_, DownloadJobRow>(
        "SELECT id, url, url_type, channel_id, status, error_message, tracks_found, tracks_imported, \
         tracks_skipped, tracks_errored, download_current_item, download_total_items, \
         download_percent, pid, created_at, updated_at \
         FROM download_jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}
