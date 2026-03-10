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

use gurujisivananda::configuration;
use gurujisivananda::import;
use sqlx::postgres::PgPoolOptions;
use std::path::PathBuf;

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
        std::fs::create_dir_all(&storage_path).expect("Failed to create audio storage directory");
        println!("Using local file storage");
        None
    };

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_lazy_with(app_config.database.connection_options());

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Could not run database migrations");

    println!("Scanning {}...", source_dir.display());

    let stats = import::import_directory(&source_dir, &storage_path, &pool, s3.as_ref()).await;

    println!();
    println!("Import complete:");
    println!("  Imported: {}", stats.imported);
    println!("  Skipped:  {} (already in database)", stats.skipped);
    println!("  Errors:   {}", stats.errors);
}
