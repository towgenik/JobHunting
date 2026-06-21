mod db;
mod generate;
mod templates;

use axum::{
    extract::{Form, Path, State},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use templates::{
    CvContent, Experience, IndexTemplate, JobRow, JobTemplate,
    ProcessingTemplate, SettingsTemplate,
};

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub db:            SqlitePool,
    pub http:          reqwest::Client,
    pub llm_endpoint:  String,
    pub llm_api_key:   String,
    pub llm_model:     String,
    pub mock_llm:      bool,
    pub openai_compat: bool,   // true = OpenAI-style (Bearer + choices[0]); false = Anthropic
}

impl AppState {
    pub fn from_env(db: SqlitePool) -> Self {
        let endpoint = std::env::var("LLM_ENDPOINT")
            .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages".into());
        // Auto-detect: explicit LLM_PROVIDER env wins; otherwise infer from endpoint URL.
        let openai_compat = std::env::var("LLM_PROVIDER")
            .map(|v| v.to_lowercase() != "anthropic")
            .unwrap_or_else(|_| !endpoint.contains("anthropic.com"));
        Self {
            db,
            http:          reqwest::Client::new(),
            llm_api_key:   std::env::var("LLM_API_KEY").unwrap_or_default(),
            llm_model:     std::env::var("LLM_MODEL")
                               .unwrap_or_else(|_| "claude-sonnet-4-6".into()),
            mock_llm:      std::env::var("LLM_MOCK").is_ok(),
            openai_compat,
            llm_endpoint:  endpoint,
        }
    }
}

// ---------------------------------------------------------------------------
// Forms
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct JobForm {
    url: String,
}

#[derive(Deserialize)]
struct DecisionForm {
    approved: Option<String>,  // "true" when approving
    reason:   Option<String>,
}

#[derive(Deserialize)]
struct SettingsForm {
    master_cv: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// ponytail: hardcoded host allowlist; replace with config-driven list in Phase 2
fn is_phase1_url(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h == "id.jobstreet.com"))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

async fn index(State(app): State<AppState>) -> impl IntoResponse {
    let rows = db::list_jobs(&app.db).await.unwrap_or_default();
    let jobs = rows
        .into_iter()
        .map(|r| JobRow { id: r.id, title: r.title, status: r.status })
        .collect();
    IndexTemplate { jobs }
}

// POST /jobs — create stub record, spawn background task, return polling card immediately.
async fn submit_job(
    State(app): State<AppState>,
    Form(body): Form<JobForm>,
) -> Response {
    if !is_phase1_url(&body.url) {
        return Html(
            "<article><span class=\"error\">Phase 1 supports id.jobstreet.com URLs only.</span></article>"
                .to_string(),
        )
        .into_response();
    }

    // Duplicate URL: surface the existing row's card rather than returning 500.
    // ponytail: check before insert so we never rely on catching the UNIQUE error
    // string (driver-specific and fragile). Two requests racing on the same URL is
    // unlikely for a single-user tool but the check is cheap.
    if let Ok(Some(existing_id)) = db::get_job_id_by_url(&app.db, &body.url).await {
        let url = db::get_job_url(&app.db, existing_id)
            .await
            .unwrap_or_else(|_| body.url.clone());
        return ProcessingTemplate { id: existing_id, url }.into_response();
    }

    let job_id = match db::create_job_stub(&app.db, &body.url).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("create_job_stub failed: {e}");
            return Html(format!(
                "<article><span class=\"error\">Failed to create job: {e}</span></article>"
            ))
            .into_response();
        }
    };

    tokio::spawn({
        let app = app.clone();
        async move {
            if let Err(e) = generate::process_job(&app, job_id).await {
                eprintln!("process_job {job_id} failed: {e}");
                let _ = db::set_status(&app.db, job_id, "failed").await;
            }
        }
    });

    ProcessingTemplate { id: job_id, url: body.url }.into_response()
}

// GET /jobs/:id/card — HTMX polls every 2s; terminal status cards drop hx-trigger.
async fn job_card(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Response {
    let status = db::get_status(&app.db, job_id)
        .await
        .unwrap_or_default();

    match status.as_str() {
        "pending_approval" => {
            db::render_cv_ready(&app.db, job_id).await.into_response()
        }
        "failed" => Html(format!(
            "<article id=\"job-{job_id}\"><span class=\"error\">Processing failed.</span></article>"
        ))
        .into_response(),
        _ => {
            let url = db::get_job_url(&app.db, job_id)
                .await
                .unwrap_or_default();
            ProcessingTemplate { id: job_id, url }.into_response()
        }
    }
}

// GET /jobs/:id — CV review page
async fn job_detail(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Response {
    let rec = match db::get_job(&app.db, job_id).await {
        Ok(r) => r,
        Err(e) => {
            return Html(format!("<p>Error loading job: {e}</p>")).into_response();
        }
    };

    // Parse the JSON CV into typed structs for the template
    let cv_val: serde_json::Value = serde_json::from_str(&rec.cv).unwrap_or_default();
    let summary = cv_val["summary"].as_str().unwrap_or("").to_string();
    let skills: Vec<String> = cv_val["skills"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let experiences: Vec<Experience> = cv_val["experiences"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|e| Experience {
                    company: e["company"].as_str().unwrap_or("").to_string(),
                    role: e["role"].as_str().unwrap_or("").to_string(),
                    bullet_points: e["bullet_points"]
                        .as_array()
                        .map(|bp| {
                            bp.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();

    JobTemplate {
        id: job_id,
        title: rec.title,
        description: rec.description,
        cv: CvContent { summary, skills, experiences },
        status: rec.status,
        reject_reason: rec.reject_reason,
    }
    .into_response()
}

// POST /jobs/:id/decision — approve or reject
async fn job_decision(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
    Form(body): Form<DecisionForm>,
) -> Response {
    let approved = body.approved.as_deref() == Some("true");
    if approved {
        let _ = db::approve_job(&app.db, job_id).await;
    } else {
        let reason = body.reason.as_deref().unwrap_or("").to_string();
        let _ = db::reject_job(&app.db, job_id, &reason).await;
    }
    // Redirect back to the job page
    axum::response::Redirect::to(&format!("/jobs/{job_id}")).into_response()
}

// GET /settings
async fn settings_page(State(app): State<AppState>) -> impl IntoResponse {
    let master_cv = db::get_master_cv(&app.db).await.unwrap_or_default();
    SettingsTemplate { master_cv }
}

// POST /settings
async fn save_settings(
    State(app): State<AppState>,
    Form(body): Form<SettingsForm>,
) -> Response {
    let _ = db::upsert_master_cv(&app.db, &body.master_cv).await;
    // hx-swap="none" on the form — just 200 OK
    Html("").into_response()
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL missing — .env not loaded?");

    let pool = SqlitePool::connect(&database_url)
        .await
        .expect("failed to connect to SQLite");

    let state = AppState::from_env(pool);

    let app = Router::new()
        .route("/",                  get(index))
        .route("/jobs",              post(submit_job))
        .route("/jobs/:id",          get(job_detail))
        .route("/jobs/:id/card",     get(job_card))
        .route("/jobs/:id/decision", post(job_decision))
        .route("/settings",          get(settings_page).post(save_settings))
        .with_state(state);

    // Bind to 0.0.0.0 so the server is reachable inside Docker (127.0.0.1 would
    // be invisible outside the container). When running natively via `make dev`
    // this is still fine — the port is only published on localhost by compose.
    let bind_addr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".into());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .unwrap();
    println!("Listening on http://{bind_addr}");
    axum::serve(listener, app).await.unwrap();
}
