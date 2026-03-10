# Guruji Sivananda

A self-hosted audio streaming app built with Leptos, Axum, and PostgreSQL. Search, stream, and download audio tracks served from your own storage.

## Prerequisites

- **Rust** (nightly) — `rustup default nightly`
- **cargo-leptos** — `cargo binstall cargo-leptos` or `cargo install cargo-leptos`
- **wasm32 target** — `rustup target add wasm32-unknown-unknown`
- **dart-sass** — `npm install -g sass` or [install standalone](https://github.com/sass/dart-sass/releases)
- **PostgreSQL 17** — running locally
- **yt-dlp** — `brew install yt-dlp` or `pip3 install yt-dlp`

## Quick Start

### 1. Create the database

```bash
psql -U postgres -h localhost -c "CREATE DATABASE gurujisivananda;"
```

### 2. Run the app

```bash
cargo leptos serve
```

The app starts at [http://localhost:3000](http://localhost:3000). Migrations run automatically on startup.

### 3. Swagger UI

API docs are at [http://localhost:3000/api/swagger-ui](http://localhost:3000/api/swagger-ui).

## Downloading Audio from YouTube

Use [yt-dlp](https://github.com/yt-dlp/yt-dlp) to download audio and metadata from a YouTube channel.

### Download an entire channel

```bash
yt-dlp \
  --extract-audio --audio-format mp3 --audio-quality 0 \
  --write-info-json --write-thumbnail \
  --output "%(channel)s/%(id)s.%(ext)s" \
  --download-archive downloaded.txt \
  --print "%(playlist_index)s/%(playlist_count)s %(title)s" \
  "https://www.youtube.com/@ChannelName"
```

This creates a folder structure:

```
ChannelName/
├── VIDEO_ID_1.mp3
├── VIDEO_ID_1.info.json
├── VIDEO_ID_1.webp
├── VIDEO_ID_2.mp3
├── VIDEO_ID_2.info.json
└── ...
```

### Flags explained

| Flag | Purpose |
|------|---------|
| `--extract-audio` | Extract audio only (no video) |
| `--audio-format mp3` | Convert to MP3 |
| `--audio-quality 0` | Best quality |
| `--write-info-json` | Save metadata (title, channel, duration, tags, thumbnail URL) |
| `--write-thumbnail` | Save thumbnail image |
| `--output "%(channel)s/%(id)s.%(ext)s"` | Organize files by channel |
| `--download-archive downloaded.txt` | Track downloaded videos to skip on re-runs |
| `--print "%(playlist_index)s/%(playlist_count)s %(title)s"` | Show overall progress |

### Download a single video

```bash
yt-dlp --extract-audio --audio-format mp3 --audio-quality 0 \
  --write-info-json \
  --output "%(channel)s/%(id)s.%(ext)s" \
  "https://www.youtube.com/watch?v=VIDEO_ID"
```

### Download a playlist

```bash
yt-dlp --extract-audio --audio-format mp3 --audio-quality 0 \
  --write-info-json \
  --output "%(channel)s/%(id)s.%(ext)s" \
  --download-archive downloaded.txt \
  "https://www.youtube.com/playlist?list=PLAYLIST_ID"
```

### Resume interrupted downloads

Just re-run the same command. The `--download-archive downloaded.txt` flag tracks what's been downloaded, so only new videos are fetched.

## Importing Audio into the App

The `import-audio` binary reads yt-dlp output and loads it into the database. Files are uploaded to S3 (production) or copied to local storage (development).

```bash
# Local development (copies files to local storage path)
cargo run --bin import-audio --features ssr -- --source-dir ./ChannelName

# With S3 (set credentials via .env or env vars)
source .env && cargo run --bin import-audio --features ssr -- --source-dir ./ChannelName
```

This will:

1. Walk the source directory for `*.info.json` files
2. Parse metadata (title, channel, duration, tags, thumbnail URL)
3. Upload the audio file to S3 (or copy to local storage if S3 is not configured)
4. Insert a row into `audio_tracks` with `ON CONFLICT DO NOTHING` for dedup

You can run it multiple times safely — duplicates are skipped automatically.

### Import output

```
Scanning ./ChannelName...
  Imported: Track Title One
  Imported: Track Title Two
  Skipped (exists): Track Title Three

Import complete:
  Imported: 2
  Skipped:  1 (already in database)
  Errors:   0
```

### Partial imports

You don't need to wait for yt-dlp to finish downloading an entire channel. Import whatever has been downloaded so far, and run the import again later for new files.

## Configuration

Configuration files are in `configuration/`:

| File | Purpose |
|------|---------|
| `base.yaml` | Default settings |
| `local.yaml` | Local development overrides |
| `production.yaml` | Production overrides |

Environment variables override YAML values using the `APP_` prefix with `__` as the nesting separator (e.g., `APP_DATABASE__HOST`).

### Audio Storage

The app supports two storage backends:

- **Local filesystem** (default for development) — files stored at `audio.storage_path`
- **DigitalOcean Spaces / S3** (production) — configured via `audio.s3` settings

When no S3 config is present, the app falls back to local filesystem.

### S3 / DigitalOcean Spaces

The AWS SDK for Rust required three workarounds for DigitalOcean Spaces compatibility:

1. **HTTP/1.1 only** — DO Spaces has HTTP/2 protocol errors on uploads. A custom `hyper-rustls` connector is built with `enable_http1()` only (no HTTP/2 ALPN negotiation).
2. **Disable request checksums** — `RequestChecksumCalculation::WhenRequired` prevents the SDK from adding CRC32 checksum headers that DO Spaces doesn't support (causes 400 Bad Request).
3. **Path-style URLs** — `force_path_style(true)` uses `endpoint/bucket/key` instead of `bucket.endpoint/key`.

S3 environment variables:

```bash
APP_AUDIO__S3__ENDPOINT=https://sgp1.digitaloceanspaces.com
APP_AUDIO__S3__BUCKET=gurujisivananda-audio
APP_AUDIO__S3__REGION=sgp1
APP_AUDIO__S3__ACCESS_KEY=<your-key>
APP_AUDIO__S3__SECRET_KEY=<your-secret>
```

### Database

Default local connection: `postgresql://postgres:password@localhost:5432/gurujisivananda`

Override individual settings via environment variables:

```bash
APP_DATABASE__HOST=myhost APP_DATABASE__PASSWORD=mypass cargo leptos serve
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/health_check` | Health check |
| `GET` | `/api/v1/tracks?q=&limit=&offset=` | Search/list tracks |
| `GET` | `/api/v1/tracks/{id}/stream` | Stream audio (supports Range requests) |
| `GET` | `/api/v1/tracks/{id}/download` | Download audio as attachment |

## Development

### Run tests

```bash
cargo test --features ssr
```

### Format code

```bash
cargo fmt
```

### Lint

```bash
cargo clippy --features ssr -- -D warnings
```

### End-to-end tests

```bash
cd end2end
npm install
npx playwright install
npx playwright test
```

## Deployment

### Docker

```bash
docker build -t gurujisivananda .
docker run -p 3000:3000 \
  -e APP_DATABASE__HOST=your-db-host \
  -e APP_DATABASE__PASSWORD=your-db-password \
  -e APP_DATABASE__DATABASE_NAME=gurujisivananda \
  -e APP_AUDIO__S3__ACCESS_KEY=your-spaces-key \
  -e APP_AUDIO__S3__SECRET_KEY=your-spaces-secret \
  gurujisivananda
```

### DigitalOcean App Platform

```bash
doctl apps create --spec spec.yaml
```

Set these environment variables in the DigitalOcean dashboard:

| Variable | Description |
|----------|-------------|
| `APP_DATABASE__HOST` | PostgreSQL host (e.g., Neon endpoint) |
| `APP_DATABASE__PORT` | PostgreSQL port (default 5432) |
| `APP_DATABASE__USERNAME` | Database user |
| `APP_DATABASE__PASSWORD` | Database password |
| `APP_DATABASE__DATABASE_NAME` | Database name |
| `APP_AUDIO__S3__ACCESS_KEY` | DO Spaces access key |
| `APP_AUDIO__S3__SECRET_KEY` | DO Spaces secret key |

## Project Structure

```
gurujisivananda/
├── configuration/          # YAML config files
├── migrations/             # SQL migrations (auto-run on startup)
├── scripts/                # DB init script for CI
├── end2end/                # Playwright E2E tests
├── style/                  # Global SCSS (reset, main entry)
├── public/                 # Static assets
├── src/
│   ├── main.rs             # Server entry point (Axum + Leptos)
│   ├── lib.rs              # Library root
│   ├── app.rs              # App shell, router, header
│   ├── configuration.rs    # Config loading (YAML + env vars)
│   ├── storage.rs          # S3 client builder (DO Spaces compatible)
│   ├── telemetry.rs        # Structured logging
│   ├── db.rs               # Database queries
│   ├── bin/
│   │   └── import_audio.rs # yt-dlp import CLI
│   ├── routes/
│   │   ├── health_check.rs # Health check endpoint
│   │   └── tracks.rs       # Track list, stream, download
│   ├── components/
│   │   ├── toast.rs        # Toast notifications
│   │   └── _toast.scss
│   └── pages/
│       ├── guruji.rs       # Main page (search, play, download)
│       └── _guruji.scss
├── Cargo.toml
├── Dockerfile
├── spec.yaml               # DigitalOcean App Platform spec
└── deny.toml               # cargo-deny audit config
```
