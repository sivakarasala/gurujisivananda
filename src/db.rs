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
