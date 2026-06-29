use axum::extract::State;
use axum::response::{Html, IntoResponse, Response};
use crate::{AppState, profile, wiki};

// POST /wiki/ingest — run ingest agent on raw/ files
pub async fn wiki_ingest(State(app): State<AppState>) -> Response {
    let dir = profile::profile_dir();
    match wiki::ingest(&app, &dir).await {
        Ok(report) => Html(format!(
            "<span style=\"color:var(--status-ok)\">{}</span>",
            report.summary()
        )).into_response(),
        Err(e) => Html(format!(
            "<span style=\"color:var(--status-err)\">Ingest failed: {}</span>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;")
        )).into_response(),
    }
}

// POST /wiki/lint — run lint and write .lint-report.md
pub async fn wiki_lint(State(_app): State<AppState>) -> Response {
    let dir = profile::profile_dir();
    match wiki::lint(&dir).await {
        Ok(()) => Html("<span style=\"color:var(--status-ok)\">Lint complete. See /wiki/lint-report.</span>".to_string()).into_response(),
        Err(e) => Html(format!(
            "<span style=\"color:var(--status-err)\">Lint failed: {}</span>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;")
        )).into_response(),
    }
}

// GET /wiki/lint-report — read the last lint report
pub async fn wiki_lint_report() -> Response {
    let dir = profile::profile_dir();
    match wiki::read_lint_report(&dir) {
        Ok(report) => Html(format!("<pre style=\"white-space:pre-wrap;font-size:.82rem\">{}</pre>",
            report.replace('&', "&amp;").replace('<', "&lt;"))).into_response(),
        Err(e) => Html(format!("<p>Error: {}</p>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;"))).into_response(),
    }
}
