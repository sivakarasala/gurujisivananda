use regex::Regex;
use std::path::PathBuf;
use std::sync::LazyLock;
use tokio::io::{AsyncBufReadExt, BufReader};
use uuid::Uuid;

// ---- yt-dlp progress parsing ----

static RE_ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[download\]\s+Downloading video (\d+) of (\d+)").unwrap());
static RE_PERCENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[download\]\s+([\d.]+)% of").unwrap());

enum YtDlpProgress {
    ItemCount { current: i32, total: i32 },
    Percent(f32),
    Other,
}

fn parse_ytdlp_line(line: &str) -> YtDlpProgress {
    if let Some(caps) = RE_ITEM.captures(line) {
        let current = caps[1].parse().unwrap_or(0);
        let total = caps[2].parse().unwrap_or(0);
        return YtDlpProgress::ItemCount { current, total };
    }
    if let Some(caps) = RE_PERCENT.captures(line) {
        let pct: f32 = caps[1].parse().unwrap_or(0.0);
        return YtDlpProgress::Percent(pct);
    }
    YtDlpProgress::Other
}

// ---- Error formatting ----

fn friendly_ytdlp_error(stderr: &str) -> String {
    // Check for common error patterns and return user-friendly messages
    let lower = stderr.to_lowercase();

    if lower.contains("js challenge") || lower.contains("js_challenge") {
        return "YouTube JS challenge failed. Try updating yt-dlp.".into();
    }
    if lower.contains("video unavailable") || lower.contains("is not available") {
        return "Video is unavailable or has been removed.".into();
    }
    if lower.contains("private video") {
        return "This video is private.".into();
    }
    if lower.contains("sign in to confirm") || lower.contains("age-restricted") {
        return "Video is age-restricted and requires sign-in.".into();
    }
    if lower.contains("copyright") {
        return "Video blocked due to copyright.".into();
    }
    if lower.contains("geo restriction") || lower.contains("not available in your country") {
        return "Video is not available in this region.".into();
    }
    if lower.contains("unable to download") && lower.contains("http error 429") {
        return "Rate limited by YouTube. Try again later.".into();
    }
    if lower.contains("http error 403") {
        return "Access forbidden by YouTube (403).".into();
    }
    if lower.contains("no video formats found") || lower.contains("unsupported url") {
        return "URL not recognized or no downloadable content found.".into();
    }
    if lower.contains("is not a valid url") {
        return "Invalid URL provided.".into();
    }
    if lower.contains("no space left on device") {
        return "Server disk full. Please free up space.".into();
    }

    // Fallback: extract the last ERROR or WARNING line from stderr
    for line in stderr.lines().rev() {
        let trimmed = line.trim();
        if let Some(msg) = trimmed.strip_prefix("ERROR:") {
            let truncated: String = msg.trim().chars().take(200).collect();
            return format!("Download error: {}", truncated);
        }
    }
    for line in stderr.lines().rev() {
        let trimmed = line.trim();
        if let Some(msg) = trimmed.strip_prefix("WARNING:") {
            let clean = msg.trim();
            // Skip noisy warnings that aren't the root cause
            if clean.starts_with("[generic]") || clean.starts_with("[download]") {
                continue;
            }
            let truncated: String = clean.chars().take(200).collect();
            return format!("Download warning: {}", truncated);
        }
    }

    // Last resort: take the last non-empty line
    if let Some(last) = stderr.lines().rev().find(|l| !l.trim().is_empty()) {
        let truncated: String = last.trim().chars().take(200).collect();
        return format!("Download failed: {}", truncated);
    }

    "Download failed unexpectedly.".into()
}

// ---- Download job runner ----

/// Run a download job: execute yt-dlp, then import the results.
pub async fn run_download_job(
    job_id: Uuid,
    url: String,
    pool: sqlx::PgPool,
    channel_id: Option<Uuid>,
) {
    let app_config = match crate::configuration::get_configuration() {
        Ok(c) => c,
        Err(e) => {
            let _ = crate::db::update_job_status(
                &pool,
                job_id,
                "failed",
                Some(&format!("Config error: {}", e)),
            )
            .await;
            return;
        }
    };

    // Check if this is a resume from paused state
    let is_resume = match crate::db::get_job_status(&pool, job_id).await {
        Ok(Some((status, _))) => status == "paused",
        _ => false,
    };

    // Update status to downloading
    let _ = crate::db::update_job_status(&pool, job_id, "downloading", None).await;

    // Create temp directory for this job (skip if resuming and dir exists)
    let temp_dir = PathBuf::from(&app_config.yt_dlp.temp_dir).join(job_id.to_string());
    if !is_resume || !temp_dir.exists() {
        if let Err(e) = tokio::fs::create_dir_all(&temp_dir).await {
            let _ = crate::db::update_job_status(
                &pool,
                job_id,
                "failed",
                Some(&format!("Failed to create temp dir: {}", e)),
            )
            .await;
            return;
        }
    }

    // Build yt-dlp command
    let output_template = temp_dir
        .join("%(id)s.%(ext)s")
        .to_string_lossy()
        .to_string();
    let mut args = vec![
        "-x".to_string(),
        "--audio-format".to_string(),
        app_config.yt_dlp.audio_format.clone(),
        "--write-info-json".to_string(),
        "--no-overwrites".to_string(),
        "--newline".to_string(), // Force progress on new lines for parsing
        "--extractor-args".to_string(),
        "youtube:js_challenge_provider=node".to_string(),
        "-o".to_string(),
        output_template,
    ];

    // Use download archive for channel syncs to skip already-downloaded videos
    if let Some(ch_id) = channel_id {
        let archive_path = temp_dir.join("archive.txt").to_string_lossy().to_string();
        args.push("--download-archive".to_string());
        args.push(archive_path);

        // Batch size: per-channel override or global default
        let batch_size = crate::db::get_channel_batch_size(&pool, ch_id)
            .await
            .unwrap_or(None)
            .unwrap_or(app_config.yt_dlp.max_downloads_per_batch as i32);
        args.push("--max-downloads".to_string());
        args.push(batch_size.to_string());
    }

    args.push(url.clone());

    tracing::info!(job_id = %job_id, url = %url, resume = is_resume, "Starting yt-dlp download");

    // Spawn yt-dlp process
    let mut child = match tokio::process::Command::new(&app_config.yt_dlp.binary_path)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Err(e) => {
            let msg = format!("yt-dlp spawn error: {}. Is yt-dlp installed?", e);
            tracing::error!(job_id = %job_id, "{}", msg);
            let _ = crate::db::update_job_status(&pool, job_id, "failed", Some(&msg)).await;
            if !is_resume {
                let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            }
            return;
        }
        Ok(c) => c,
    };

    // Store PID in DB so pause can find it
    if let Some(pid) = child.id() {
        let _ = crate::db::update_job_pid(&pool, job_id, Some(pid as i32)).await;
    }

    // Merge both stdout and stderr into one channel for progress parsing.
    // yt-dlp may write progress to either stream depending on version/flags.
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let (line_tx, mut line_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Read stdout
    let stdout_tx = line_tx.clone();
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = stdout_tx.send(line);
        }
    });

    // Read stderr (also collect full output for error reporting)
    let stderr_tx = line_tx;
    let stderr_handle = tokio::spawn(async move {
        let mut buf = String::new();
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            buf.push_str(&line);
            buf.push('\n');
            let _ = stderr_tx.send(line);
        }
        buf
    });

    let mut current_item: i32 = 0;
    let mut total_items: i32 = 0;
    let mut current_percent: f32 = 0.0;
    let mut last_db_update = std::time::Instant::now();

    // Race line reading against child exit. yt-dlp's subprocess (ffmpeg) may
    // inherit pipe fds, keeping them open after yt-dlp exits. We use select!
    // to detect when the child process itself exits, then drain remaining lines.
    let mut child_done = false;
    let mut exit_status: Result<std::process::ExitStatus, std::io::Error> =
        Err(std::io::Error::other("not started"));

    loop {
        tokio::select! {
            line = line_rx.recv(), if !child_done => {
                match line {
                    Some(line) => {
                        match parse_ytdlp_line(&line) {
                            YtDlpProgress::ItemCount { current, total } => {
                                current_item = current;
                                total_items = total;
                                current_percent = 0.0;
                            }
                            YtDlpProgress::Percent(pct) => {
                                current_percent = pct;
                            }
                            YtDlpProgress::Other => {}
                        }

                        if last_db_update.elapsed() >= std::time::Duration::from_secs(1) {
                            let _ = crate::db::update_job_download_progress(
                                &pool, job_id, current_item, total_items, current_percent,
                            ).await;
                            last_db_update = std::time::Instant::now();
                        }
                    }
                    None => break, // Channel closed, all senders dropped
                }
            }
            status = child.wait(), if !child_done => {
                exit_status = status;
                #[allow(unused_assignments)]
                {
                    child_done = true;
                }
                tracing::info!(job_id = %job_id, "yt-dlp process exited, draining remaining output");
                // Drain remaining buffered lines with a timeout
                while let Ok(Some(line)) = tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    line_rx.recv(),
                ).await {
                    match parse_ytdlp_line(&line) {
                        YtDlpProgress::ItemCount { current, total } => {
                            current_item = current;
                            total_items = total;
                        }
                        YtDlpProgress::Percent(pct) => {
                            let _ = current_percent;
                            current_percent = pct;
                        }
                        YtDlpProgress::Other => {}
                    }
                }
                break;
            }
        }
    }

    // Clear PID from DB
    let _ = crate::db::update_job_pid(&pool, job_id, None).await;

    // Final progress flush
    let _ =
        crate::db::update_job_download_progress(&pool, job_id, current_item, total_items, 100.0)
            .await;

    let stderr_output = stderr_handle.await.unwrap_or_default();

    tracing::info!(job_id = %job_id, exit = ?exit_status, stderr_len = stderr_output.len(), "yt-dlp process exited");

    match exit_status {
        Err(e) => {
            let msg = format!("yt-dlp process error: {}", e);
            tracing::error!(job_id = %job_id, "{}", msg);
            let _ = crate::db::update_job_status(&pool, job_id, "failed", Some(&msg)).await;
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            return;
        }
        Ok(status) if !status.success() => {
            // Check if this was a pause (SIGTERM) — don't fail if status is "paused"
            if let Ok(Some((db_status, _))) = crate::db::get_job_status(&pool, job_id).await {
                if db_status == "paused" {
                    tracing::info!(job_id = %job_id, "yt-dlp terminated for pause");
                    return; // Exit gracefully; resume will re-invoke
                }
            }

            // Exit code 101 = MaxDownloadsReached — expected for batched channel syncs
            let is_max_downloads = status.code() == Some(101);
            if is_max_downloads {
                tracing::info!(job_id = %job_id, "yt-dlp reached max downloads limit (expected for batch)");
            } else {
                tracing::error!(job_id = %job_id, exit_code = ?status.code(), stderr = %stderr_output.chars().take(1000).collect::<String>(), "yt-dlp failed");
                let user_msg = friendly_ytdlp_error(&stderr_output);
                let _ =
                    crate::db::update_job_status(&pool, job_id, "failed", Some(&user_msg)).await;
                let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                return;
            }
        }
        Ok(_) => {
            tracing::info!(job_id = %job_id, "yt-dlp download completed");
        }
    }

    // Count info.json files
    let tracks_found = count_info_json_files(&temp_dir) as i32;
    let _ = crate::db::update_job_progress(&pool, job_id, tracks_found, 0, 0, 0).await;

    // Log temp dir contents for debugging
    tracing::info!(job_id = %job_id, tracks_found = tracks_found, temp_dir = %temp_dir.display(), "Starting import phase");
    if let Ok(entries) = std::fs::read_dir(&temp_dir) {
        for entry in entries.flatten() {
            tracing::info!(job_id = %job_id, file = %entry.path().display(), "Temp dir file");
        }
    }

    // Import phase
    let _ = crate::db::update_job_status(&pool, job_id, "importing", None).await;

    let storage_path = PathBuf::from(&app_config.audio.storage_path);

    // Ensure local storage dir exists if not using S3
    if app_config.audio.s3.is_none() {
        let _ = std::fs::create_dir_all(&storage_path);
    }

    let s3 = if let Some(s3_settings) = &app_config.audio.s3 {
        let client = crate::storage::build_s3_client(s3_settings).await;
        Some((client, s3_settings.bucket.clone()))
    } else {
        None
    };

    let stats = crate::import::import_directory(&temp_dir, &storage_path, &pool, s3.as_ref()).await;

    // Update final progress
    let _ = crate::db::update_job_progress(
        &pool,
        job_id,
        tracks_found,
        stats.imported as i32,
        stats.skipped as i32,
        stats.errors as i32,
    )
    .await;

    // Update channel last_synced_at if this was a channel sync
    if let Some(ch_id) = channel_id {
        let _ = crate::db::update_channel_synced(&pool, ch_id).await;
    }

    let _ = crate::db::update_job_status(&pool, job_id, "completed", None).await;

    // Clean up temp directory
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    tracing::info!(
        job_id = %job_id,
        imported = stats.imported,
        skipped = stats.skipped,
        errors = stats.errors,
        "Download job completed"
    );
}

fn count_info_json_files(dir: &PathBuf) -> usize {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().is_file()
                && e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.ends_with(".info.json"))
                    .unwrap_or(false)
        })
        .count()
}

/// Start the auto-sync scheduler that periodically syncs all channels with auto_sync=true.
pub fn start_auto_sync_scheduler(pool: sqlx::PgPool) {
    tokio::spawn(async move {
        let interval_hours = crate::configuration::get_configuration()
            .map(|c| c.yt_dlp.sync_interval_hours)
            .unwrap_or(24);

        let interval = std::time::Duration::from_secs(interval_hours * 3600);
        tracing::info!(
            "Auto-sync scheduler started (interval: {} hours)",
            interval_hours
        );

        loop {
            tokio::time::sleep(interval).await;

            tracing::info!("Auto-sync: checking for channels to sync");

            let channels = match crate::db::list_channels_for_sync(&pool).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Auto-sync: failed to list channels: {}", e);
                    continue;
                }
            };

            for ch in channels {
                let job_id = Uuid::new_v4();
                match crate::db::create_download_job(
                    &pool,
                    job_id,
                    &ch.youtube_url,
                    "channel",
                    Some(ch.id),
                    None,
                )
                .await
                {
                    Ok(()) => {
                        tracing::info!(
                            channel = %ch.name,
                            job_id = %job_id,
                            "Auto-sync: created download job"
                        );
                        let pool2 = pool.clone();
                        let url = ch.youtube_url.clone();
                        let ch_id = ch.id;
                        tokio::spawn(async move {
                            run_download_job(job_id, url, pool2, Some(ch_id)).await;
                        });
                    }
                    Err(e) => {
                        tracing::error!(channel = %ch.name, "Auto-sync: failed to create job: {}", e);
                    }
                }
            }
        }
    });
}
