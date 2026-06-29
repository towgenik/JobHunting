//! SSE event bus and helpers.
//!
//! ponytail: the event_bus is a broadcast::Sender<String> on AppState.
//! publish_job_update sends a JSON message; sse_events subscribes.

use axum::extract::State;
use axum::response::sse::{Event, Sse};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::AppState;

/// Publish a job status change via SSE. The message is a JSON string
/// that clients use to update the specific job card without polling.
pub fn publish_job_update(app: &AppState, job_id: Uuid, status: &str, progress: &str) {
    let msg = serde_json::json!({"id": job_id.to_string(), "status": status, "progress": progress}).to_string();
    let _ = app.event_bus.send(msg);
}

/// GET /events — SSE stream of job status updates. Replaces 2s polling.
pub async fn sse_events(
    State(app): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = app.event_bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|r| match r {
        Ok(msg) => Some(Ok(Event::default().data(msg))),
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}
