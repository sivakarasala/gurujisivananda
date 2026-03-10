use crate::components::use_toast;
use crate::pages::login::get_current_user;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DownloadJob {
    pub id: String,
    pub url: String,
    pub url_type: String,
    pub status: String,
    pub error_message: Option<String>,
    pub tracks_found: i32,
    pub tracks_imported: i32,
    pub tracks_skipped: i32,
    pub tracks_errored: i32,
    pub download_current_item: i32,
    pub download_total_items: i32,
    pub download_percent: f32,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub youtube_url: String,
    pub auto_sync: bool,
    pub last_synced_at: Option<String>,
    pub batch_size: Option<i32>,
    pub track_count: i64,
}

#[cfg(feature = "ssr")]
fn classify_youtube_url(url: &str) -> Result<String, ServerFnError> {
    if url.contains("/playlist?list=") || url.contains("&list=") {
        Ok("playlist".into())
    } else if url.contains("/channel/")
        || url.contains("/@")
        || url.contains("/c/")
        || url.contains("/user/")
    {
        Ok("channel".into())
    } else if url.contains("youtube.com/watch") || url.contains("youtu.be/") {
        Ok("video".into())
    } else {
        Err(ServerFnError::new(
            "Unrecognized YouTube URL. Please use a video, channel, or playlist URL.",
        ))
    }
}

#[server]
pub async fn start_download(url: String) -> Result<DownloadJob, ServerFnError> {
    let user = crate::auth::require_admin().await?;
    let pool = crate::db::db().await?;

    let url_type = classify_youtube_url(&url)?;
    let job_id = uuid::Uuid::new_v4();

    crate::db::create_download_job(&pool, job_id, &url, &url_type, None, Some(user.id))
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    tracing::info!(job_id = %job_id, url = %url, url_type = %url_type, user = %user.email, "Download started");

    let pool2 = pool.clone();
    let url2 = url.clone();
    tokio::spawn(async move {
        crate::jobs::run_download_job(job_id, url2, pool2, None).await;
    });

    Ok(DownloadJob {
        id: job_id.to_string(),
        url,
        url_type,
        status: "pending".into(),
        error_message: None,
        tracks_found: 0,
        tracks_imported: 0,
        tracks_skipped: 0,
        tracks_errored: 0,
        download_current_item: 0,
        download_total_items: 0,
        download_percent: 0.0,
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string(),
    })
}

#[server]
pub async fn list_jobs() -> Result<Vec<DownloadJob>, ServerFnError> {
    crate::auth::require_admin().await?;
    let pool = crate::db::db().await?;

    let rows = crate::db::list_download_jobs(&pool, 50)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|r| DownloadJob {
            id: r.id.to_string(),
            url: r.url,
            url_type: r.url_type,
            status: r.status,
            error_message: r.error_message,
            tracks_found: r.tracks_found,
            tracks_imported: r.tracks_imported,
            tracks_skipped: r.tracks_skipped,
            tracks_errored: r.tracks_errored,
            download_current_item: r.download_current_item,
            download_total_items: r.download_total_items,
            download_percent: r.download_percent,
            created_at: r.created_at.format("%Y-%m-%d %H:%M").to_string(),
        })
        .collect())
}

#[server]
pub async fn add_channel(name: String, youtube_url: String) -> Result<Channel, ServerFnError> {
    crate::auth::require_admin().await?;
    let pool = crate::db::db().await?;

    let id = uuid::Uuid::new_v4();
    crate::db::create_channel(&pool, id, &name, &youtube_url)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    tracing::info!(channel_id = %id, name = %name, url = %youtube_url, "Channel added");

    Ok(Channel {
        id: id.to_string(),
        name,
        youtube_url,
        auto_sync: true,
        last_synced_at: None,
        batch_size: None,
        track_count: 0,
    })
}

#[server]
pub async fn remove_channel(channel_id: String) -> Result<(), ServerFnError> {
    crate::auth::require_admin().await?;
    let pool = crate::db::db().await?;

    let id = uuid::Uuid::parse_str(&channel_id).map_err(|e| ServerFnError::new(e.to_string()))?;

    crate::db::delete_channel(&pool, id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    tracing::info!(channel_id = %id, "Channel removed");

    Ok(())
}

#[server]
pub async fn sync_channel(channel_id: String) -> Result<DownloadJob, ServerFnError> {
    let user = crate::auth::require_admin().await?;
    let pool = crate::db::db().await?;

    let ch_id =
        uuid::Uuid::parse_str(&channel_id).map_err(|e| ServerFnError::new(e.to_string()))?;

    // Look up channel URL
    let channels = crate::db::list_channels(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let channel = channels
        .into_iter()
        .find(|c| c.id == ch_id)
        .ok_or_else(|| ServerFnError::new("Channel not found"))?;

    let job_id = uuid::Uuid::new_v4();
    crate::db::create_download_job(
        &pool,
        job_id,
        &channel.youtube_url,
        "channel",
        Some(ch_id),
        Some(user.id),
    )
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    tracing::info!(job_id = %job_id, channel = %channel.name, channel_id = %ch_id, user = %user.email, "Channel sync started");

    let pool2 = pool.clone();
    let url = channel.youtube_url.clone();
    tokio::spawn(async move {
        crate::jobs::run_download_job(job_id, url, pool2, Some(ch_id)).await;
    });

    Ok(DownloadJob {
        id: job_id.to_string(),
        url: channel.youtube_url,
        url_type: "channel".into(),
        status: "pending".into(),
        error_message: None,
        tracks_found: 0,
        tracks_imported: 0,
        tracks_skipped: 0,
        tracks_errored: 0,
        download_current_item: 0,
        download_total_items: 0,
        download_percent: 0.0,
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string(),
    })
}

#[server]
pub async fn get_channels() -> Result<Vec<Channel>, ServerFnError> {
    crate::auth::require_admin().await?;
    let pool = crate::db::db().await?;

    let rows = crate::db::list_channels(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let track_counts = crate::db::count_tracks_by_channel(&pool)
        .await
        .unwrap_or_default();

    Ok(rows
        .into_iter()
        .map(|r| {
            let count = track_counts.get(&r.id).copied().unwrap_or(0);
            Channel {
                id: r.id.to_string(),
                name: r.name,
                youtube_url: r.youtube_url,
                auto_sync: r.auto_sync,
                last_synced_at: r
                    .last_synced_at
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string()),
                batch_size: r.max_downloads_per_batch,
                track_count: count,
            }
        })
        .collect())
}

#[server]
pub async fn pause_job(job_id: String) -> Result<(), ServerFnError> {
    crate::auth::require_admin().await?;
    let pool = crate::db::db().await?;

    let id = uuid::Uuid::parse_str(&job_id).map_err(|e| ServerFnError::new(e.to_string()))?;

    let (status, pid) = crate::db::get_job_status(&pool, id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("Job not found"))?;

    if status != "downloading" {
        return Err(ServerFnError::new("Job is not currently downloading"));
    }

    // Mark as paused BEFORE killing, so the job runner detects it on exit
    crate::db::update_job_status(&pool, id, "paused", None)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    // Send SIGTERM to the yt-dlp process
    if let Some(pid) = pid {
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
    }

    // Clear the PID
    crate::db::update_job_pid(&pool, id, None)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    #[cfg(feature = "ssr")]
    {
        use crate::events::{job_event_sender, JobEvent};
        let _ = job_event_sender().send(JobEvent::StatusChanged {
            job_id: id.to_string(),
            status: "paused".into(),
            error_message: None,
        });
    }

    tracing::info!(job_id = %id, "Job paused");
    Ok(())
}

#[server]
pub async fn resume_job(job_id: String) -> Result<(), ServerFnError> {
    let user = crate::auth::require_admin().await?;
    let pool = crate::db::db().await?;

    let id = uuid::Uuid::parse_str(&job_id).map_err(|e| ServerFnError::new(e.to_string()))?;

    let (status, _) = crate::db::get_job_status(&pool, id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("Job not found"))?;

    if status != "paused" {
        return Err(ServerFnError::new("Job is not paused"));
    }

    let job_row = crate::db::get_download_job(&pool, id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("Job not found"))?;

    tracing::info!(job_id = %id, user = %user.email, "Resuming job");

    let pool2 = pool.clone();
    let url = job_row.url.clone();
    let channel_id = job_row.channel_id;
    tokio::spawn(async move {
        crate::jobs::run_download_job(id, url, pool2, channel_id).await;
    });

    Ok(())
}

#[server]
pub async fn cancel_job(job_id: String) -> Result<(), ServerFnError> {
    crate::auth::require_admin().await?;
    let pool = crate::db::db().await?;

    let id = uuid::Uuid::parse_str(&job_id).map_err(|e| ServerFnError::new(e.to_string()))?;

    let (status, pid) = crate::db::get_job_status(&pool, id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("Job not found"))?;

    if status != "downloading" && status != "pending" && status != "importing" {
        return Err(ServerFnError::new("Job is not active"));
    }

    // Kill the yt-dlp process if running
    if let Some(pid) = pid {
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    }

    crate::db::update_job_status(&pool, id, "failed", Some("Cancelled by admin"))
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    crate::db::update_job_pid(&pool, id, None)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    #[cfg(feature = "ssr")]
    {
        use crate::events::{job_event_sender, JobEvent};
        let _ = job_event_sender().send(JobEvent::StatusChanged {
            job_id: id.to_string(),
            status: "failed".into(),
            error_message: Some("Cancelled by admin".into()),
        });
    }

    tracing::info!(job_id = %id, "Job cancelled by admin");
    Ok(())
}

#[server]
pub async fn update_batch_size(
    channel_id: String,
    batch_size: String,
) -> Result<(), ServerFnError> {
    crate::auth::require_admin().await?;
    let pool = crate::db::db().await?;

    let id = uuid::Uuid::parse_str(&channel_id).map_err(|e| ServerFnError::new(e.to_string()))?;

    let size = if batch_size.trim().is_empty() {
        None
    } else {
        let n: i32 = batch_size
            .trim()
            .parse()
            .map_err(|_| ServerFnError::new("Invalid number"))?;
        if n < 1 {
            return Err(ServerFnError::new("Batch size must be at least 1"));
        }
        Some(n)
    };

    crate::db::update_channel_batch_size(&pool, id, size)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    tracing::info!(channel_id = %id, batch_size = ?size, "Channel batch size updated");
    Ok(())
}

#[component]
pub fn AdminPage() -> impl IntoView {
    let toast = use_toast();

    let current_user = Resource::new(|| (), |_| get_current_user());

    let url_input = RwSignal::new(String::new());
    let channel_name = RwSignal::new(String::new());
    let channel_url = RwSignal::new(String::new());
    let jobs = RwSignal::new(Vec::<DownloadJob>::new());
    let channels = RwSignal::new(Vec::<Channel>::new());

    let download_action = ServerAction::<StartDownload>::new();
    let download_pending = download_action.pending();

    let add_channel_action = ServerAction::<AddChannel>::new();
    let add_channel_pending = add_channel_action.pending();

    // Load initial data on mount
    let list_jobs_action = ServerAction::<ListJobs>::new();
    let get_channels_action = ServerAction::<GetChannels>::new();

    Effect::new(move || {
        list_jobs_action.dispatch(ListJobs {});
        get_channels_action.dispatch(GetChannels {});
    });

    // Handle job list responses
    let list_jobs_value = list_jobs_action.value();
    Effect::new(move || {
        if let Some(Ok(j)) = list_jobs_value.get() {
            jobs.set(j);
        }
    });

    // Handle channel list responses
    let channels_value = get_channels_action.value();
    Effect::new(move || {
        if let Some(Ok(c)) = channels_value.get() {
            channels.set(c);
        }
    });

    // Handle download started
    let download_value = download_action.value();
    Effect::new(move || {
        if let Some(result) = download_value.get() {
            match result {
                Ok(job) => {
                    toast.success(format!("Download started for {} URL", job.url_type));
                    url_input.set(String::new());
                    // Refresh job list
                    list_jobs_action.dispatch(ListJobs {});
                }
                Err(e) => {
                    toast.error(format!("Failed to start download: {}", e));
                }
            }
        }
    });

    // Handle channel added
    let add_channel_value = add_channel_action.value();
    Effect::new(move || {
        if let Some(result) = add_channel_value.get() {
            match result {
                Ok(_) => {
                    toast.success("Channel added".to_string());
                    channel_name.set(String::new());
                    channel_url.set(String::new());
                    get_channels_action.dispatch(GetChannels {});
                }
                Err(e) => {
                    toast.error(format!("Failed to add channel: {}", e));
                }
            }
        }
    });

    // SSE: listen for real-time job updates instead of polling
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;

        let es = web_sys::EventSource::new("/api/v1/jobs/sse").expect("EventSource");
        let es_cleanup = es.clone();

        let on_message =
            Closure::<dyn Fn(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
                list_jobs_action.dispatch(ListJobs {});
                // Refresh channels when a job completes (track counts change)
                if let Some(data) = event.data().as_string() {
                    if data.contains("\"status\":\"completed\"") {
                        get_channels_action.dispatch(GetChannels {});
                    }
                }
            });
        es.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();

        leptos::prelude::on_cleanup(move || {
            es_cleanup.close();
        });
    }

    let on_download = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let url = url_input.get_untracked();
        if !url.is_empty() {
            download_action.dispatch(StartDownload { url });
        }
    };

    let on_add_channel = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let name = channel_name.get_untracked();
        let url = channel_url.get_untracked();
        if !name.is_empty() && !url.is_empty() {
            add_channel_action.dispatch(AddChannel {
                name,
                youtube_url: url,
            });
        }
    };

    view! {
        <Suspense fallback=|| view! { <div class="admin-page"><p>"Loading..."</p></div> }>
            {move || {
                current_user.get().map(|result| {
                    match result {
                        Ok(Some(user)) if user.role == "admin" => {
                            view! {
                                <div class="admin-page">
                                    <h1>"Admin Dashboard"</h1>

                                    // Section 1: Quick Download
                                    <section class="admin-section">
                                        <h2>"Quick Download"</h2>
                                        <p class="section-desc">"Paste a YouTube video, channel, or playlist URL"</p>
                                        <form class="download-form" on:submit=on_download>
                                            <input
                                                type="url"
                                                placeholder="https://youtube.com/watch?v=... or https://youtube.com/@channel"
                                                required
                                                prop:value=move || url_input.get()
                                                on:input=move |e| url_input.set(event_target_value(&e))
                                            />
                                            <button type="submit" disabled=move || download_pending.get()>
                                                {move || if download_pending.get() { "Starting..." } else { "Download" }}
                                            </button>
                                        </form>
                                    </section>

                                    // Section 2: Channels
                                    <section class="admin-section">
                                        <h2>"Channels"</h2>
                                        <p class="section-desc">"Manage saved channels for syncing"</p>
                                        <form class="add-channel-form" on:submit=on_add_channel>
                                            <input
                                                type="text"
                                                placeholder="Channel name"
                                                required
                                                prop:value=move || channel_name.get()
                                                on:input=move |e| channel_name.set(event_target_value(&e))
                                            />
                                            <input
                                                type="url"
                                                placeholder="https://youtube.com/@channel"
                                                required
                                                prop:value=move || channel_url.get()
                                                on:input=move |e| channel_url.set(event_target_value(&e))
                                            />
                                            <button type="submit" disabled=move || add_channel_pending.get()>
                                                {move || if add_channel_pending.get() { "Adding..." } else { "Add Channel" }}
                                            </button>
                                        </form>

                                        <div class="channel-list">
                                            <For
                                                each=move || channels.get()
                                                key=|ch| format!("{}-{}-{:?}-{}", ch.id, ch.track_count, ch.batch_size, ch.last_synced_at.as_deref().unwrap_or(""))
                                                children=move |ch| {
                                                    let ch_id = ch.id.clone();
                                                    let ch_id_sync = ch.id.clone();
                                                    let ch_id_batch = ch.id.clone();
                                                    let sync_action = ServerAction::<SyncChannel>::new();
                                                    let remove_action = ServerAction::<RemoveChannel>::new();
                                                    let batch_action = ServerAction::<UpdateBatchSize>::new();

                                                    let sync_pending = sync_action.pending();
                                                    let remove_pending = remove_action.pending();
                                                    let batch_pending = batch_action.pending();

                                                    let batch_input = RwSignal::new(
                                                        ch.batch_size.map(|n| n.to_string()).unwrap_or_default()
                                                    );

                                                    let track_count_text = if ch.track_count > 0 {
                                                        format!("{} tracks synced", ch.track_count)
                                                    } else {
                                                        "No tracks synced".to_string()
                                                    };

                                                    // Handle sync result
                                                    let sync_value = sync_action.value();
                                                    Effect::new(move || {
                                                        if let Some(result) = sync_value.get() {
                                                            match result {
                                                                Ok(_) => {
                                                                    toast.success("Sync started".to_string());
                                                                    list_jobs_action.dispatch(ListJobs {});
                                                                }
                                                                Err(e) => toast.error(format!("Sync failed: {}", e)),
                                                            }
                                                        }
                                                    });

                                                    // Handle remove result
                                                    let remove_value = remove_action.value();
                                                    Effect::new(move || {
                                                        if let Some(result) = remove_value.get() {
                                                            match result {
                                                                Ok(()) => {
                                                                    toast.success("Channel removed".to_string());
                                                                    get_channels_action.dispatch(GetChannels {});
                                                                }
                                                                Err(e) => toast.error(format!("Remove failed: {}", e)),
                                                            }
                                                        }
                                                    });

                                                    // Handle batch size result
                                                    let batch_value = batch_action.value();
                                                    Effect::new(move || {
                                                        if let Some(result) = batch_value.get() {
                                                            match result {
                                                                Ok(()) => {
                                                                    toast.success("Batch size updated".to_string());
                                                                    get_channels_action.dispatch(GetChannels {});
                                                                }
                                                                Err(e) => { toast.error(format!("Update failed: {}", e)); }
                                                            }
                                                        }
                                                    });

                                                    view! {
                                                        <div class="channel-card">
                                                            <div class="channel-info">
                                                                <span class="channel-name">{ch.name.clone()}</span>
                                                                <span class="channel-url">{ch.youtube_url.clone()}</span>
                                                                <span class="channel-synced">
                                                                    {ch.last_synced_at.clone().map(|d| format!("Last synced: {}", d)).unwrap_or_else(|| "Never synced".to_string())}
                                                                </span>
                                                                <span class="channel-tracks">{track_count_text}</span>
                                                                <div class="channel-batch">
                                                                    <label>"Batch:"</label>
                                                                    <input
                                                                        type="number"
                                                                        min="1"
                                                                        placeholder="50 (default)"
                                                                        prop:value=move || batch_input.get()
                                                                        on:input=move |e| batch_input.set(event_target_value(&e))
                                                                    />
                                                                    <button
                                                                        class="batch-save-btn"
                                                                        disabled=move || batch_pending.get()
                                                                        on:click=move |_| {
                                                                            batch_action.dispatch(UpdateBatchSize {
                                                                                channel_id: ch_id_batch.clone(),
                                                                                batch_size: batch_input.get_untracked(),
                                                                            });
                                                                        }
                                                                    >
                                                                        {move || if batch_pending.get() { "..." } else { "Save" }}
                                                                    </button>
                                                                </div>
                                                            </div>
                                                            <div class="channel-actions">
                                                                <button
                                                                    class="sync-btn"
                                                                    disabled=move || sync_pending.get()
                                                                    on:click=move |_| {
                                                                        sync_action.dispatch(SyncChannel { channel_id: ch_id_sync.clone() });
                                                                    }
                                                                >
                                                                    {move || if sync_pending.get() { "Syncing..." } else { "Sync Now" }}
                                                                </button>
                                                                <button
                                                                    class="remove-btn"
                                                                    disabled=move || remove_pending.get()
                                                                    on:click=move |_| {
                                                                        remove_action.dispatch(RemoveChannel { channel_id: ch_id.clone() });
                                                                    }
                                                                >
                                                                    "Remove"
                                                                </button>
                                                            </div>
                                                        </div>
                                                    }
                                                }
                                            />
                                        </div>
                                    </section>

                                    // Section 3: Download Jobs
                                    <section class="admin-section">
                                        <h2>"Download Jobs"</h2>
                                        <div class="jobs-list">
                                            <For
                                                each=move || jobs.get()
                                                key=|job| format!("{}-{}-{}-{:.0}", job.id, job.status, job.download_current_item, job.download_percent)
                                                children=move |job| {
                                                    let status_class = match job.status.as_str() {
                                                        "pending" => "status-pending",
                                                        "downloading" => "status-downloading",
                                                        "importing" => "status-importing",
                                                        "completed" => "status-completed",
                                                        "failed" => "status-failed",
                                                        "paused" => "status-paused",
                                                        _ => "status-pending",
                                                    };

                                                    let progress_text = if job.tracks_found > 0 {
                                                        format!(
                                                            "{}/{} imported{}",
                                                            job.tracks_imported,
                                                            job.tracks_found,
                                                            if job.tracks_errored > 0 {
                                                                format!(", {} errors", job.tracks_errored)
                                                            } else {
                                                                String::new()
                                                            }
                                                        )
                                                    } else {
                                                        String::new()
                                                    };

                                                    let truncated_url = if job.url.len() > 60 {
                                                        format!("{}...", &job.url[..60])
                                                    } else {
                                                        job.url.clone()
                                                    };

                                                    // Progress bar calculations
                                                    let show_progress_bar = job.status == "downloading" || job.status == "paused";
                                                    let is_paused_bar = job.status == "paused";
                                                    let overall_pct = if job.download_total_items > 0 {
                                                        let completed = (job.download_current_item - 1).max(0) as f32;
                                                        let total = job.download_total_items as f32;
                                                        let file_frac = job.download_percent / 100.0;
                                                        ((completed + file_frac) / total * 100.0).min(100.0)
                                                    } else if job.download_percent > 0.0 {
                                                        job.download_percent
                                                    } else {
                                                        0.0
                                                    };
                                                    let item_text = if job.download_total_items > 0 {
                                                        format!("Item {} of {}", job.download_current_item, job.download_total_items)
                                                    } else if show_progress_bar {
                                                        "Downloading...".to_string()
                                                    } else {
                                                        String::new()
                                                    };
                                                    let pct_str = format!("{:.0}%", overall_pct);
                                                    let width_style = format!("width: {:.1}%", overall_pct);
                                                    let bar_class = if is_paused_bar { "job-progress-bar-fill paused" } else { "job-progress-bar-fill" };

                                                    // Pause/resume/cancel state
                                                    let is_downloading = job.status == "downloading";
                                                    let is_paused = job.status == "paused";
                                                    let is_active = job.status == "downloading" || job.status == "pending" || job.status == "importing";
                                                    let job_id_for_pause = job.id.clone();
                                                    let job_id_for_resume = job.id.clone();
                                                    let job_id_for_cancel = job.id.clone();

                                                    let pause_action = ServerAction::<PauseJob>::new();
                                                    let resume_action = ServerAction::<ResumeJob>::new();
                                                    let cancel_action = ServerAction::<CancelJob>::new();
                                                    let pause_pending = pause_action.pending();
                                                    let resume_pending = resume_action.pending();
                                                    let cancel_pending = cancel_action.pending();

                                                    let pause_value = pause_action.value();
                                                    Effect::new(move || {
                                                        if let Some(result) = pause_value.get() {
                                                            match result {
                                                                Ok(()) => { list_jobs_action.dispatch(ListJobs {}); }
                                                                Err(e) => { toast.error(format!("Pause failed: {}", e)); }
                                                            }
                                                        }
                                                    });
                                                    let resume_value = resume_action.value();
                                                    Effect::new(move || {
                                                        if let Some(result) = resume_value.get() {
                                                            match result {
                                                                Ok(()) => { list_jobs_action.dispatch(ListJobs {}); }
                                                                Err(e) => { toast.error(format!("Resume failed: {}", e)); }
                                                            }
                                                        }
                                                    });
                                                    let cancel_value = cancel_action.value();
                                                    Effect::new(move || {
                                                        if let Some(result) = cancel_value.get() {
                                                            match result {
                                                                Ok(()) => {
                                                                    toast.success("Job cancelled".to_string());
                                                                    list_jobs_action.dispatch(ListJobs {});
                                                                }
                                                                Err(e) => { toast.error(format!("Cancel failed: {}", e)); }
                                                            }
                                                        }
                                                    });

                                                    view! {
                                                        <div class="job-card">
                                                            <div class="job-header">
                                                                <span class=format!("job-status {}", status_class)>
                                                                    {job.status.clone()}
                                                                </span>
                                                                <span class="job-type">{job.url_type.clone()}</span>
                                                                <span class="job-time">{job.created_at.clone()}</span>
                                                            </div>
                                                            <div class="job-url">{truncated_url}</div>
                                                            {show_progress_bar.then(|| view! {
                                                                <div class="job-progress-bar-wrapper">
                                                                    <div class="job-progress-bar">
                                                                        <div class=bar_class style=width_style></div>
                                                                    </div>
                                                                    <div class="job-progress-bar-text">
                                                                        <span>{item_text.clone()}</span>
                                                                        <span>{pct_str.clone()}</span>
                                                                    </div>
                                                                </div>
                                                            })}
                                                            {(!progress_text.is_empty()).then(|| view! {
                                                                <div class="job-progress">{progress_text.clone()}</div>
                                                            })}
                                                            {job.error_message.clone().map(|msg| view! {
                                                                <div class="job-error">{msg}</div>
                                                            })}
                                                            {(is_downloading || is_paused || is_active).then(move || view! {
                                                                <div class="job-actions">
                                                                    {is_downloading.then(|| {
                                                                        let jid = job_id_for_pause.clone();
                                                                        view! {
                                                                            <button
                                                                                class="pause-btn"
                                                                                disabled=move || pause_pending.get()
                                                                                on:click=move |_| {
                                                                                    pause_action.dispatch(PauseJob { job_id: jid.clone() });
                                                                                }
                                                                            >
                                                                                {move || if pause_pending.get() { "Pausing..." } else { "Pause" }}
                                                                            </button>
                                                                        }
                                                                    })}
                                                                    {is_paused.then(|| {
                                                                        let jid = job_id_for_resume.clone();
                                                                        view! {
                                                                            <button
                                                                                class="resume-btn"
                                                                                disabled=move || resume_pending.get()
                                                                                on:click=move |_| {
                                                                                    resume_action.dispatch(ResumeJob { job_id: jid.clone() });
                                                                                }
                                                                            >
                                                                                {move || if resume_pending.get() { "Resuming..." } else { "Resume" }}
                                                                            </button>
                                                                        }
                                                                    })}
                                                                    {is_active.then(|| {
                                                                        let jid = job_id_for_cancel.clone();
                                                                        view! {
                                                                            <button
                                                                                class="cancel-btn"
                                                                                disabled=move || cancel_pending.get()
                                                                                on:click=move |_| {
                                                                                    cancel_action.dispatch(CancelJob { job_id: jid.clone() });
                                                                                }
                                                                            >
                                                                                {move || if cancel_pending.get() { "Cancelling..." } else { "Cancel" }}
                                                                            </button>
                                                                        }
                                                                    })}
                                                                </div>
                                                            })}
                                                        </div>
                                                    }
                                                }
                                            />
                                        </div>
                                    </section>
                                </div>
                            }.into_any()
                        }
                        Ok(Some(_)) => {
                            view! {
                                <div class="admin-page">
                                    <p>"Access denied. Admin privileges required."</p>
                                </div>
                            }.into_any()
                        }
                        _ => {
                            // Not logged in - redirect to login
                            #[cfg(feature = "hydrate")]
                            {
                                let window = leptos::web_sys::window().unwrap();
                                let _ = window.location().set_href("/login");
                            }
                            view! {
                                <div class="admin-page">
                                    <p>"Redirecting to login..."</p>
                                </div>
                            }.into_any()
                        }
                    }
                })
            }}
        </Suspense>
    }
}
