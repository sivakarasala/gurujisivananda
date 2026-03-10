# Admin Panel

The admin panel (`/admin`) provides authenticated management of YouTube audio downloads, channels, and sync jobs.

## Authentication

- Login at `/login` with email and password (Argon2-hashed)
- Session-based auth via `session_id` cookie (expires after set duration)
- Admin user is seeded on startup from `APP_ADMIN_EMAIL` and `APP_ADMIN_PASSWORD` env vars
- All admin endpoints call `require_admin()` to enforce role-based access

## Features

### Direct URL Downloads

Paste any YouTube URL to start a download job:
- **Video** — single video (`youtube.com/watch?v=...` or `youtu.be/...`)
- **Playlist** — all videos in a playlist (`/playlist?list=...`)
- **Channel** — all videos from a channel (`/@handle`, `/channel/...`, `/c/...`)

The URL type is auto-detected. Each download spawns a background `yt-dlp` process that:
1. Downloads video and extracts audio to MP3 (configurable format)
2. Writes `.info.json` metadata per track
3. Imports tracks into the database with deduplication (`ON CONFLICT youtube_id DO NOTHING`)
4. Copies audio files to local storage or uploads to S3

### Channel Management

Channels are saved YouTube channel URLs that can be repeatedly synced.

- **Add Channel** — provide a name and YouTube URL
- **Sync Now** — triggers a download job for the channel
- **Remove** — deletes the channel (does not remove already-imported tracks)
- **Batch Size** — configurable per-channel limit on videos downloaded per sync (default: 50, from `yt_dlp.max_downloads_per_batch` in config). Set to limit yt-dlp's `--max-downloads` flag. Empty = use global default.
- **Track Count** — shows total tracks imported from completed jobs for this channel
- **Last Synced** — timestamp of the most recent completed sync

### Auto-Sync Scheduler

A background scheduler runs on a configurable interval (default: 24 hours, from `yt_dlp.sync_interval_hours`). It syncs all channels with `auto_sync = true` by creating download jobs automatically.

### Download Jobs

Each download creates a job with real-time progress tracking:

**Statuses:** `pending` → `downloading` → `importing` → `completed` (or `failed`, `paused`)

**Progress tracking:**
- yt-dlp stdout/stderr is parsed for `[download] Downloading video X of Y` and `[download] XX.X% of` patterns
- Progress is written to the database every second (throttled)
- UI polls every 5 seconds while jobs are active
- Progress bar shows overall completion across all items in the batch

**Pause/Resume:**
- Downloading jobs can be paused (sends `SIGTERM` to yt-dlp process)
- Paused jobs can be resumed (re-spawns yt-dlp with `--no-overwrites` to skip completed files)

**Error handling:**
- yt-dlp errors are mapped to user-friendly messages (JS challenge, unavailable, private, age-restricted, copyright, geo-restricted, rate-limited, disk full, etc.)
- Exit code 101 (`MaxDownloadsReached`) is treated as success for batched channel syncs
- Fallback extracts `ERROR:` or `WARNING:` lines from yt-dlp output

### Import Pipeline

After yt-dlp finishes, the import phase:
1. Walks the temp directory for `.info.json` files
2. Finds the corresponding audio file (`.mp3`, `.m4a`, `.opus`, `.webm`, `.ogg`)
3. Uploads to S3 or copies to local storage (skips if file already exists at destination)
4. Inserts into `audio_tracks` table (skips duplicates by `youtube_id`)
5. Updates job progress with imported/skipped/errored counts
6. Cleans up the temp directory

## Configuration

Relevant settings in `configuration/base.yaml`:

```yaml
yt_dlp:
  binary_path: "yt-dlp"           # Path to yt-dlp binary
  temp_dir: "/tmp/yt-dlp-downloads" # Temp dir for downloads
  audio_format: "mp3"              # Audio format for extraction
  sync_interval_hours: 24          # Auto-sync interval
  max_downloads_per_batch: 50      # Default batch size for channel syncs

audio:
  storage_path: "./data/audio"     # Local audio file storage
  # s3:                            # Optional S3 storage
  #   bucket: "..."
  #   region: "..."
```

## Database Tables

- **`users`** — admin accounts (email, password_hash, role)
- **`sessions`** — login sessions (user_id, expires_at)
- **`channels`** — saved YouTube channels (name, youtube_url, auto_sync, max_downloads_per_batch)
- **`download_jobs`** — download job state and progress (url, status, tracks_found/imported/skipped/errored, download progress, pid)
- **`audio_tracks`** — imported audio metadata (youtube_id, title, channel, duration, file_path, thumbnail_url)

## Key Files

| File | Description |
|------|-------------|
| `src/pages/admin.rs` | Admin page component and server functions |
| `src/jobs.rs` | yt-dlp job runner, progress parsing, auto-sync scheduler |
| `src/import.rs` | Audio file import and S3 upload logic |
| `src/auth.rs` | Authentication, password hashing, session management |
| `src/db.rs` | All database queries |
| `src/pages/_admin.scss` | Admin page styles |
