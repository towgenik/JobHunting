use axum::{
    extract::{Form, State},
    response::{Html, IntoResponse, Response},
};
use crate::{AppState, profile, templates::ProfileTemplate};
use super::forms::ProfileForm;

// GET /profile — file explorer + editor + A4 preview
pub async fn profile_page(
    State(_app): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let files = profile::list_profile_files().unwrap_or_default();
    let current_file = params.get("file").cloned().unwrap_or_else(|| "index.md".into());
    let content = profile::read_profile_file(&current_file).unwrap_or_default();
    ProfileTemplate { files, current_file, content }.into_response()
}

// POST /profile — save edited file to disk
pub async fn profile_save(
    State(_app): State<AppState>,
    Form(body): Form<ProfileForm>,
) -> Response {
    let file = body.file.trim().to_string();
    if file.is_empty() || file.contains("..") {
        return Html("<span style=\"color:var(--status-err)\">Invalid file path.</span>").into_response();
    }
    match profile::write_profile_file(&file, &body.content) {
        Ok(()) => {
            // If index.md was saved, sync to DB too
            if file == "index.md" {
                let _ = profile::sync_profile_to_db(&_app.db).await;
            }
            Html("<span style=\"color:var(--status-ok)\">Saved.</span>").into_response()
        }
        Err(e) => Html(format!("<span style=\"color:var(--status-err)\">Save failed: {}</span>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;"))).into_response(),
    }
}

// POST /profile/sync — force re-sync profile from files to DB.
pub async fn profile_sync(State(app): State<AppState>) -> Response {
    match profile::sync_profile_to_db(&app.db).await {
        Ok(()) => Html("<span style=\"color:var(--status-ok)\">Profile synced from files.</span>".to_string()).into_response(),
        Err(e) => Html(format!("<span style=\"color:var(--status-err)\">Sync failed: {}</span>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;"))).into_response(),
    }
}

