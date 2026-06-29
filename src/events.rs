//! SSE event bus and helpers.
//!
//! ponytail: the event_bus is a broadcast::Sender<String> on AppState.
//! All events use a JSON envelope with "kind" discriminator so the frontend
//! can dispatch by event type (job-update, crawl-progress, etc.).

use axum::extract::State;
use axum::response::sse::{Event, Sse};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::AppState;

/// Core publish helper — adds "kind" field to every message.
fn publish(app: &AppState, kind: &str, mut data: serde_json::Value) {
    data["kind"] = serde_json::Value::String(kind.into());
    let msg = data.to_string();
    let _ = app.event_bus.send(msg);
}

/// Publish a job status change via SSE.
pub fn publish_job_update(app: &AppState, job_id: Uuid, status: &str, progress: &str) {
    publish(app, "job-update", serde_json::json!({
        "id": job_id.to_string(), "status": status, "progress": progress,
    }));
}

/// Publish crawl progress (per-spawn activity message).
pub fn publish_crawl_progress(app: &AppState, message: &str) {
    publish(app, "crawl-progress", serde_json::json!({
        "message": message,
    }));
}

/// Publish crawl finished (crawl loop ended).
pub fn publish_crawl_finished(app: &AppState) {
    publish(app, "crawl-finished", serde_json::json!({}));
}

/// Publish scheduler run finished (background run completed).
pub fn publish_scheduler_run_finished(app: &AppState, run_id: i64, status: &str) {
    publish(app, "scheduler-finished", serde_json::json!({
        "run_id": run_id, "status": status,
    }));
}

/// GET /events — SSE stream of all events. Frontend dispatches by "kind".
pub async fn sse_events(
    State(app): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = app.event_bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|r| match r {
        Ok(msg) => Some(Ok(Event::default().event("job-update").data(msg))),
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}
