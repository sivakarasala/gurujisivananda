mod health_check;
mod tracks;

pub use health_check::health_check;
pub use tracks::{download_track, list_tracks, stream_track, AppState};

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        health_check::health_check,
        tracks::list_tracks,
        tracks::stream_track,
        tracks::download_track,
    ),
    components(schemas(tracks::TrackListItem)),
    tags(
        (name = "v1", description = "API v1 endpoints")
    )
)]
pub struct ApiDoc;
