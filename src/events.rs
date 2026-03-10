use serde::Serialize;
use std::sync::LazyLock;
use tokio::sync::broadcast;

/// Payload sent over the broadcast channel when job state changes.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum JobEvent {
    DownloadProgress {
        job_id: String,
        current_item: i32,
        total_items: i32,
        percent: f32,
    },
    StatusChanged {
        job_id: String,
        status: String,
        error_message: Option<String>,
    },
    ImportProgress {
        job_id: String,
        tracks_found: i32,
        tracks_imported: i32,
        tracks_skipped: i32,
        tracks_errored: i32,
    },
}

/// Global broadcast sender. Capacity of 64 is generous; receivers that fall
/// behind will get `RecvError::Lagged` and simply skip missed events.
static JOB_EVENTS: LazyLock<broadcast::Sender<JobEvent>> = LazyLock::new(|| {
    let (tx, _) = broadcast::channel(64);
    tx
});

/// Get a reference to the global sender (used by jobs.rs to emit events).
pub fn job_event_sender() -> &'static broadcast::Sender<JobEvent> {
    &JOB_EVENTS
}

/// Subscribe to job events (used by the SSE endpoint handler).
pub fn subscribe_job_events() -> broadcast::Receiver<JobEvent> {
    JOB_EVENTS.subscribe()
}
