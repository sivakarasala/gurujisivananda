use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client as S3Client;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
pub struct YtDlpInfo {
    pub id: String,
    pub title: String,
    pub channel: Option<String>,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub upload_date: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub thumbnail: Option<String>,
}

#[derive(Default)]
pub struct ImportStats {
    pub imported: u32,
    pub skipped: u32,
    pub errors: u32,
}

pub fn find_audio_file(info_path: &Path) -> Option<PathBuf> {
    let stem = info_path.file_stem()?.to_str()?;
    let base = stem.strip_suffix(".info").unwrap_or(stem);
    let dir = info_path.parent()?;

    for ext in &["mp3", "m4a", "opus", "webm", "ogg"] {
        let candidate = dir.join(format!("{}.{}", base, ext));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

pub fn sanitize_channel(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

pub fn parse_upload_date(s: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(s, "%Y%m%d").ok()
}

pub async fn s3_object_exists(client: &S3Client, bucket: &str, key: &str) -> bool {
    client
        .head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .is_ok()
}

/// Process a single info.json file. Returns Ok(true) if imported, Ok(false) if skipped.
pub async fn process_entry(
    info_path: &Path,
    storage_path: &Path,
    pool: &sqlx::PgPool,
    s3: Option<&(S3Client, String)>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(info_path)?;
    let info: YtDlpInfo = serde_json::from_str(&content)?;

    let audio_file =
        find_audio_file(info_path).ok_or_else(|| format!("No audio file found for {}", info.id))?;

    let channel = info
        .channel
        .or(info.uploader)
        .unwrap_or_else(|| "unknown".to_string());

    let channel_slug = sanitize_channel(&channel);
    let audio_ext = audio_file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("mp3");
    let dest_relative = format!("{}/{}.{}", channel_slug, info.id, audio_ext);

    let file_size = if let Some((client, bucket)) = s3 {
        if s3_object_exists(client, bucket, &dest_relative).await {
            let head = client
                .head_object()
                .bucket(bucket)
                .key(&dest_relative)
                .send()
                .await?;
            head.content_length().unwrap_or(0)
        } else {
            let body = ByteStream::from_path(&audio_file).await?;
            let local_size = std::fs::metadata(&audio_file)?.len() as i64;
            client
                .put_object()
                .bucket(bucket)
                .key(&dest_relative)
                .body(body)
                .content_type("audio/mpeg")
                .send()
                .await?;
            local_size
        }
    } else {
        let dest_full = storage_path.join(&dest_relative);
        if let Some(parent) = dest_full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if !dest_full.exists() {
            std::fs::copy(&audio_file, &dest_full)?;
        }
        std::fs::metadata(&dest_full)?.len() as i64
    };

    let duration_seconds = info.duration.unwrap_or(0.0) as i32;
    let upload_date = info.upload_date.as_deref().and_then(parse_upload_date);
    let description = info.description.unwrap_or_default();
    let tags = info.tags.unwrap_or_default();
    let thumbnail_url = info.thumbnail.unwrap_or_default();

    let result = sqlx::query(
        "INSERT INTO audio_tracks (id, youtube_id, title, channel, duration_seconds, upload_date, description, tags, thumbnail_url, file_path, file_size) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
         ON CONFLICT (youtube_id) DO NOTHING"
    )
    .bind(uuid::Uuid::new_v4())
    .bind(&info.id)
    .bind(&info.title)
    .bind(&channel)
    .bind(duration_seconds)
    .bind(upload_date)
    .bind(&description)
    .bind(&tags)
    .bind(&thumbnail_url)
    .bind(&dest_relative)
    .bind(file_size)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        println!("  Skipped (exists): {}", info.title);
        Ok(false)
    } else {
        println!("  Imported: {}", info.title);
        Ok(true)
    }
}

/// Import all info.json files from a directory. Returns aggregate stats.
pub async fn import_directory(
    source_dir: &Path,
    storage_path: &Path,
    pool: &sqlx::PgPool,
    s3: Option<&(S3Client, String)>,
) -> ImportStats {
    let mut stats = ImportStats::default();

    for entry in walkdir::WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if !filename.ends_with(".info.json") {
            continue;
        }

        match process_entry(path, storage_path, pool, s3).await {
            Ok(true) => {
                tracing::info!(file = %path.display(), "Imported track");
                stats.imported += 1;
            }
            Ok(false) => {
                tracing::info!(file = %path.display(), "Skipped track (already exists)");
                stats.skipped += 1;
            }
            Err(e) => {
                tracing::error!(file = %path.display(), error = %e, "Failed to import track");
                stats.errors += 1;
            }
        }
    }

    stats
}
