use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use aws_sdk_s3::Client as S3Client;
use sqlx::PgPool;
use utoipa::ToSchema;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub audio_storage_path: String,
    pub s3_client: Option<S3Client>,
    pub s3_bucket: Option<String>,
}

#[derive(Deserialize)]
pub struct TrackQuery {
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct TrackListItem {
    pub id: String,
    pub youtube_id: String,
    pub title: String,
    pub channel: String,
    pub duration_seconds: i32,
    pub thumbnail_url: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/tracks",
    tag = "v1",
    params(
        ("q" = Option<String>, Query, description = "Search query (full-text on title)"),
        ("limit" = Option<i64>, Query, description = "Max results (default 20)"),
        ("offset" = Option<i64>, Query, description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "List of tracks", body = Vec<TrackListItem>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_tracks(
    State(state): State<AppState>,
    Query(params): Query<TrackQuery>,
) -> Response {
    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0);

    let rows = if let Some(q) = &params.q {
        if q.trim().is_empty() {
            crate::db::list_tracks(&state.pool, limit, offset).await
        } else {
            crate::db::search_tracks(&state.pool, q, limit, offset).await
        }
    } else {
        crate::db::list_tracks(&state.pool, limit, offset).await
    };

    match rows {
        Ok(tracks) => {
            let items: Vec<TrackListItem> = tracks
                .into_iter()
                .map(|t| TrackListItem {
                    id: t.id.to_string(),
                    youtube_id: t.youtube_id,
                    title: t.title,
                    channel: t.channel,
                    duration_seconds: t.duration_seconds,
                    thumbnail_url: t.thumbnail_url,
                })
                .collect();
            Json(items).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to query tracks: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to query tracks").into_response()
        }
    }
}

/// Parse a Range header like "bytes=0-" or "bytes=100-200".
fn parse_range(header: &str, total: u64) -> Option<(u64, u64)> {
    let s = header.strip_prefix("bytes=")?;
    let mut parts = s.splitn(2, '-');
    let start: u64 = parts.next()?.parse().ok()?;
    let end_str = parts.next()?;
    let end = if end_str.is_empty() {
        total.checked_sub(1)?
    } else {
        end_str.parse::<u64>().ok()?.min(total.checked_sub(1)?)
    };
    (start <= end && start < total).then_some((start, end))
}

#[utoipa::path(
    get,
    path = "/api/v1/tracks/{id}/stream",
    tag = "v1",
    params(
        ("id" = String, Path, description = "Track UUID")
    ),
    responses(
        (status = 200, description = "Audio file stream"),
        (status = 206, description = "Partial audio content"),
        (status = 404, description = "Track not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn stream_track(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    let track = match crate::db::get_track_by_id(&state.pool, id).await {
        Ok(Some(t)) => t,
        Ok(None) => return (StatusCode::NOT_FOUND, "Track not found").into_response(),
        Err(e) => {
            tracing::error!("DB error: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let range_header = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let (Some(client), Some(bucket)) = (&state.s3_client, &state.s3_bucket) {
        stream_from_s3(client, bucket, &track, range_header).await
    } else {
        stream_from_local(&state.audio_storage_path, &track, range_header).await
    }
}

async fn stream_from_s3(
    client: &S3Client,
    bucket: &str,
    track: &crate::db::TrackRow,
    range_header: Option<String>,
) -> Response {
    let mut request = client
        .get_object()
        .bucket(bucket)
        .key(&track.file_path);

    if let Some(range) = &range_header {
        request = request.range(range.clone());
    }

    let result = match request.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("S3 GetObject error for {}: {:?}", track.file_path, e);
            return (StatusCode::NOT_FOUND, "Audio file not found").into_response();
        }
    };

    let content_length = result.content_length().unwrap_or(0) as u64;
    let content_range = result.content_range().map(|s| s.to_string());
    let bytes = match result.body.collect().await {
        Ok(aggregated) => aggregated.into_bytes(),
        Err(e) => {
            tracing::error!("S3 read error for {}: {:?}", track.file_path, e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read audio").into_response();
        }
    };
    let body = axum::body::Body::from(bytes);

    if range_header.is_some() {
        let content_range = content_range.unwrap_or_default();

        Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_TYPE, "audio/mpeg")
            .header(header::CONTENT_LENGTH, content_length)
            .header(header::CONTENT_RANGE, content_range)
            .header(header::ACCEPT_RANGES, "bytes")
            .body(body)
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "audio/mpeg")
            .header(header::CONTENT_LENGTH, content_length)
            .header(header::ACCEPT_RANGES, "bytes")
            .body(body)
            .unwrap()
    }
}

async fn stream_from_local(
    storage_path: &str,
    track: &crate::db::TrackRow,
    range_header: Option<String>,
) -> Response {
    let full_path = format!("{}/{}", storage_path, track.file_path);
    let file = match tokio::fs::File::open(&full_path).await {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("File not found: {} — {:?}", full_path, e);
            return (StatusCode::NOT_FOUND, "Audio file not found").into_response();
        }
    };

    let file_size = track.file_size as u64;

    if let Some(range_str) = &range_header {
        if let Some((start, end)) = parse_range(range_str, file_size) {
            let length = end - start + 1;

            use tokio::io::{AsyncReadExt, AsyncSeekExt};
            let mut file = file;
            if let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await {
                tracing::error!("Seek error: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "Seek error").into_response();
            }
            let limited = file.take(length);
            let stream = tokio_util::io::ReaderStream::new(limited);
            let body = axum::body::Body::from_stream(stream);

            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, "audio/mpeg")
                .header(header::CONTENT_LENGTH, length)
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {}-{}/{}", start, end, file_size),
                )
                .header(header::ACCEPT_RANGES, "bytes")
                .body(body)
                .unwrap();
        }
    }

    let stream = tokio_util::io::ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "audio/mpeg")
        .header(header::CONTENT_LENGTH, file_size)
        .header(header::ACCEPT_RANGES, "bytes")
        .body(body)
        .unwrap()
}

#[utoipa::path(
    get,
    path = "/api/v1/tracks/{id}/download",
    tag = "v1",
    params(
        ("id" = String, Path, description = "Track UUID")
    ),
    responses(
        (status = 200, description = "Audio file download"),
        (status = 404, description = "Track not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn download_track(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    let track = match crate::db::get_track_by_id(&state.pool, id).await {
        Ok(Some(t)) => t,
        Ok(None) => return (StatusCode::NOT_FOUND, "Track not found").into_response(),
        Err(e) => {
            tracing::error!("DB error: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let safe_title: String = track
        .title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect();
    let filename = format!("{}.mp3", safe_title);

    if let (Some(client), Some(bucket)) = (&state.s3_client, &state.s3_bucket) {
        download_from_s3(client, bucket, &track, &filename).await
    } else {
        download_from_local(&state.audio_storage_path, &track, &filename).await
    }
}

async fn download_from_s3(
    client: &S3Client,
    bucket: &str,
    track: &crate::db::TrackRow,
    filename: &str,
) -> Response {
    let result = match client
        .get_object()
        .bucket(bucket)
        .key(&track.file_path)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("S3 GetObject error for {}: {:?}", track.file_path, e);
            return (StatusCode::NOT_FOUND, "Audio file not found").into_response();
        }
    };

    let content_length = result.content_length().unwrap_or(0);
    let bytes = match result.body.collect().await {
        Ok(aggregated) => aggregated.into_bytes(),
        Err(e) => {
            tracing::error!("S3 read error for {}: {:?}", track.file_path, e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read audio").into_response();
        }
    };
    let body = axum::body::Body::from(bytes);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "audio/mpeg")
        .header(header::CONTENT_LENGTH, content_length)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(body)
        .unwrap()
}

async fn download_from_local(
    storage_path: &str,
    track: &crate::db::TrackRow,
    filename: &str,
) -> Response {
    let full_path = format!("{}/{}", storage_path, track.file_path);
    let file = match tokio::fs::File::open(&full_path).await {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("File not found: {} — {:?}", full_path, e);
            return (StatusCode::NOT_FOUND, "Audio file not found").into_response();
        }
    };

    let stream = tokio_util::io::ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "audio/mpeg")
        .header(header::CONTENT_LENGTH, track.file_size)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(body)
        .unwrap()
}
