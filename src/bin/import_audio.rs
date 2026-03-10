//! Import audio files downloaded by yt-dlp into the database and storage directory.
//!
//! Usage:
//!   cargo run --bin import-audio --features ssr -- --source-dir ./ChannelName
//!
//! Expects yt-dlp output with `--write-info-json`:
//!   ChannelName/
//!   ├── VIDEO_ID.mp3
//!   ├── VIDEO_ID.info.json
//!   └── ...

use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client as S3Client;
use gurujisivananda::configuration;
use serde::Deserialize;
use sqlx::postgres::PgPoolOptions;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct YtDlpInfo {
    id: String,
    title: String,
    channel: Option<String>,
    uploader: Option<String>,
    duration: Option<f64>,
    upload_date: Option<String>,
    description: Option<String>,
    tags: Option<Vec<String>>,
    thumbnail: Option<String>,
}

struct ImportStats {
    imported: u32,
    skipped: u32,
    errors: u32,
}

fn parse_args() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    let mut source_dir = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--source-dir" => {
                i += 1;
                if i < args.len() {
                    source_dir = Some(PathBuf::from(&args[i]));
                }
            }
            _ => {
                // Treat positional arg as source dir
                source_dir = Some(PathBuf::from(&args[i]));
            }
        }
        i += 1;
    }

    source_dir.unwrap_or_else(|| {
        eprintln!("Usage: import-audio --source-dir <path>");
        eprintln!("  <path>  Directory containing yt-dlp output (*.info.json + *.mp3)");
        std::process::exit(1);
    })
}

fn find_audio_file(info_path: &Path) -> Option<PathBuf> {
    let stem = info_path.file_stem()?.to_str()?;
    // yt-dlp names files as ID.info.json, so strip ".info"
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

fn sanitize_channel(name: &str) -> String {
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

fn parse_upload_date(s: &str) -> Option<chrono::NaiveDate> {
    // yt-dlp format: "20230415"
    chrono::NaiveDate::parse_from_str(s, "%Y%m%d").ok()
}

#[tokio::main]
async fn main() {
    let source_dir = parse_args();

    if !source_dir.exists() {
        eprintln!(
            "Error: source directory does not exist: {}",
            source_dir.display()
        );
        std::process::exit(1);
    }

    let app_config = configuration::get_configuration().expect("Failed to read configuration");
    let storage_path = PathBuf::from(&app_config.audio.storage_path);

    let s3 = if let Some(s3_settings) = &app_config.audio.s3 {
        let client = gurujisivananda::storage::build_s3_client(s3_settings).await;
        println!("S3 storage enabled (bucket: {})", s3_settings.bucket);
        Some((client, s3_settings.bucket.clone()))
    } else {
        // Ensure local storage directory exists
        std::fs::create_dir_all(&storage_path).expect("Failed to create audio storage directory");
        println!("Using local file storage");
        None
    };

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_lazy_with(app_config.database.connection_options());

    // Run migrations
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Could not run database migrations");

    println!("Scanning {}...", source_dir.display());

    let mut stats = ImportStats {
        imported: 0,
        skipped: 0,
        errors: 0,
    };

    // Walk the directory for *.info.json files
    for entry in walkdir::WalkDir::new(&source_dir)
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

        match process_entry(path, &storage_path, &pool, s3.as_ref()).await {
            Ok(true) => {
                stats.imported += 1;
            }
            Ok(false) => {
                stats.skipped += 1;
            }
            Err(e) => {
                eprintln!("  Error processing {}: {}", path.display(), e);
                stats.errors += 1;
            }
        }
    }

    println!();
    println!("Import complete:");
    println!("  Imported: {}", stats.imported);
    println!("  Skipped:  {} (already in database)", stats.skipped);
    println!("  Errors:   {}", stats.errors);
}

/// Process a single info.json file. Returns Ok(true) if imported, Ok(false) if skipped.
async fn process_entry(
    info_path: &Path,
    storage_path: &Path,
    pool: &sqlx::PgPool,
    s3: Option<&(S3Client, String)>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(info_path)?;
    let info: YtDlpInfo = serde_json::from_str(&content)?;

    // Find the corresponding audio file
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
        // Upload to S3 (skip if already exists)
        if s3_object_exists(client, bucket, &dest_relative).await {
            // Get size from S3
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
        // Copy to local storage
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

async fn s3_object_exists(client: &S3Client, bucket: &str, key: &str) -> bool {
    client
        .head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .is_ok()
}
