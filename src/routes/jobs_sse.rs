use axum::response::sse::{Event, KeepAlive, Sse};
use std::convert::Infallible;
use tokio_stream::StreamExt;

pub async fn job_events_sse() -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = crate::events::subscribe_job_events();

    let stream =
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| match result {
            Ok(event) => {
                let json = serde_json::to_string(&event).ok()?;
                Some(Ok(Event::default().data(json)))
            }
            Err(_) => None, // Lagged — skip missed events
        });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
