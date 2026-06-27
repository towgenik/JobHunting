mod crawler;
mod db;
mod generate;
mod profile;
mod templates;

use axum::{
    extract::{Form, Path, State},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use uuid::Uuid;

use templates::{
    CvContent, CvPrintTemplate, Experience, IndexTemplate, JobRow, JobTemplate,
    ProcessingTemplate, SearchCardTemplate, SettingsTemplate,
    ReviewSummary, Verification, VerificationItem, RankSummary,
    SearchQueriesTemplate,
};

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub db:               SqlitePool,
    pub http:             reqwest::Client,
    pub llm_endpoint:     String,
    pub llm_api_key:      String,
    pub llm_model:        String,
    pub mock_llm:         bool,
    pub openai_compat:    bool,   // true = OpenAI-style (Bearer + choices[0]); false = Anthropic
    pub llm_semaphore:    std::sync::Arc<tokio::sync::Semaphore>,
    pub scheduler_running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub profile_title_blacklist: Vec<String>,
    pub profile_deal_breaker_keywords: Vec<String>,
}

/// Parse profile/index.md YAML frontmatter for pre-filter configuration.
/// Returns (title_blacklist, deal_breaker_keywords).
fn parse_profile_frontmatter() -> (Vec<String>, Vec<String>) {
    let default_blacklist = vec![
        "WordPress".into(), "PHP".into(), "Frontend".into(),
        "Mobile".into(), "Game Dev".into(), "Blockchain".into(),
        "Crypto".into(), "Gambling".into(),
    ];
    let default_deal_breakers = vec![
        "mandatory office".into(), "5 days in office".into(), "on-site only".into(),
    ];

    let dir = std::env::var("PROFILE_DIR").unwrap_or_else(|_| "./profile".into());
    let content = match std::fs::read_to_string(format!("{dir}/index.md")) {
        Ok(c) => c,
        Err(_) => return (default_blacklist, default_deal_breakers),
    };

    // Extract YAML frontmatter between --- delimiters
    let fm = match content.strip_prefix("---") {
        Some(rest) => match rest.find("---") {
            Some(end) => &rest[..end],
            None => return (default_blacklist, default_deal_breakers),
        },
        None => return (default_blacklist, default_deal_breakers),
    };

    let mut title_blacklist = default_blacklist;
    let mut deal_breaker_keywords = default_deal_breakers;

    // Simple line-based YAML parsing (same approach as settings.html JS)
    let mut current_key = String::new();
    let mut in_list = false;

    for line in fm.lines() {
        let trimmed = line.trim();

        // Check for key: value
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let value = value.trim();

            if value.is_empty() || value == "[" {
                // List follows
                current_key = key.to_string();
                in_list = true;
                continue;
            }

            // Inline list: key: [item1, item2]
            if value.starts_with('[') && value.ends_with(']') {
                let items: Vec<String> = value[1..value.len()-1]
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                match key {
                    "title_blacklist" => title_blacklist = items,
                    "deal_breaker_keywords" => deal_breaker_keywords = items,
                    _ => {}
                }
                in_list = false;
                continue;
            }

            // Simple value
            in_list = false;
            continue;
        }

        // List item: - value
        if in_list && trimmed.starts_with('-') {
            let item = trimmed[1..].trim().trim_matches('"').trim_matches('\'').to_string();
            if !item.is_empty() {
                match current_key.as_str() {
                    "title_blacklist" => title_blacklist.push(item),
                    "deal_breaker_keywords" => deal_breaker_keywords.push(item),
                    _ => {}
                }
            }
        }
    }

    (title_blacklist, deal_breaker_keywords)
}

impl AppState {
    pub fn from_env(db: SqlitePool) -> Self {
        let endpoint = std::env::var("LLM_ENDPOINT")
            .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages".into());
        // Auto-detect: explicit LLM_PROVIDER env wins; otherwise infer from endpoint URL.
        let openai_compat = std::env::var("LLM_PROVIDER")
            .map(|v| v.to_lowercase() != "anthropic")
            .unwrap_or_else(|_| !endpoint.contains("anthropic.com"));
        let concurrency: usize = std::env::var("LLM_CONCURRENCY")
            .ok().and_then(|s| s.parse().ok()).unwrap_or(2);

        // Parse profile frontmatter for pre-filters
        let (profile_title_blacklist, profile_deal_breaker_keywords) =
            parse_profile_frontmatter();

        Self {
            db,
            http:             reqwest::Client::new(),
            llm_api_key:      std::env::var("LLM_API_KEY").unwrap_or_default(),
            llm_model:        std::env::var("LLM_MODEL")
                                  .unwrap_or_else(|_| "claude-sonnet-4-6".into()),
            mock_llm:         std::env::var("LLM_MOCK").is_ok(),
            openai_compat,
            llm_endpoint:     endpoint,
            llm_semaphore:    std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency)),
            scheduler_running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            profile_title_blacklist,
            profile_deal_breaker_keywords,
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
    approved:        Option<String>,  // "true" when approving
    reason:          Option<String>,
    review_notes:    Option<String>,
    decision_reason: Option<String>, // quality_gap, already_applied, location, salary, company, other
}

#[derive(Deserialize)]
struct SettingsForm {
    master_cv: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// ponytail: hardcoded host allowlist; replace with config-driven list in Phase 2
fn is_jobstreet_url(url: &str) -> bool {
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
    if !is_jobstreet_url(&body.url) {
        return Html(
            "<article><span class=\"error\">Only id.jobstreet.com URLs are supported.</span></article>"
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

// POST /jobs/search — accept a listing URL, crawl for detail URLs, process each.
async fn submit_search(
    State(app): State<AppState>,
    Form(body): Form<JobForm>,
) -> Response {
    if !is_jobstreet_url(&body.url) {
        return Html(
            "<article><span class=\"error\">Only id.jobstreet.com URLs are supported.</span></article>"
                .to_string(),
        )
        .into_response();
    }

    let search_id = Uuid::new_v4();
    tokio::spawn({
        let app = app.clone();
        let url = body.url.clone();
        async move {
            crawler::run_search(app, search_id, url).await;
        }
    });

    // Return a polling card immediately.
    SearchCardTemplate { search_id, url: body.url, terminal: 0, total: 0 }.into_response()
}

// GET /searches/:id/card — HTMX polls every 3s until all jobs reach terminal status.
async fn search_card(
    State(app): State<AppState>,
    Path(search_id): Path<Uuid>,
) -> Response {
    let (terminal, total) = db::get_search_progress(&app.db, search_id)
        .await
        .unwrap_or((0, 0));

    let url = db::get_search_url(&app.db, search_id)
        .await
        .unwrap_or_default();

    SearchCardTemplate { search_id, url, terminal, total }.into_response()
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

    // Parse review
    let review = rec.review_feedback.as_deref().map(|feedback| {
        ReviewSummary {
            score: rec.review_score.unwrap_or(0),
            feedback: feedback.to_string(),
        }
    });

    // Parse verification
    let verification = rec.verification.as_deref().and_then(|s| {
        serde_json::from_str::<serde_json::Value>(s).ok().map(|v| {
            let items = v["items"].as_array().map(|arr| {
                arr.iter().map(|it| VerificationItem {
                    category: it["category"].as_str().unwrap_or("").to_string(),
                    field:    it["field"].as_str().unwrap_or("").to_string(),
                    claim:    it["claim"].as_str().unwrap_or("").to_string(),
                    verdict:  it["verdict"].as_str().unwrap_or("").to_string(),
                    evidence: it["evidence"].as_str().unwrap_or("").to_string(),
                }).collect()
            }).unwrap_or_default();

            Verification {
                truth_pct: v["truth_pct"].as_i64().unwrap_or(0),
                items,
                gap_report: v["gap_report"].as_str().unwrap_or("").to_string(),
                fabrication_detected: v["fabrication_detected"].as_bool().unwrap_or(false),
                incomplete: v["incomplete"].as_bool().unwrap_or(false),
            }
        })
    });

    // Parse rank
    let rank = rec.rank.as_deref().and_then(|s| {
        serde_json::from_str::<serde_json::Value>(s).ok().map(|v| {
            RankSummary {
                approval_probability: v["approval_probability"].as_i64().unwrap_or(0),
                good: v["good"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default(),
                bad: v["bad"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default(),
                improvements: v["improvements"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default(),
            }
        })
    });

    JobTemplate {
        id: job_id,
        title: rec.title,
        description: rec.description,
        cv: CvContent { summary, skills, experiences },
        status: rec.status,
        reject_reason: rec.reject_reason,
        review,
        verification,
        rank,
        review_notes: rec.review_notes.unwrap_or_default(),
    }
    .into_response()
}

// GET /jobs/:id/cv — print-optimized standalone CV page (browser print → PDF)
async fn cv_print(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Response {
    let rec = match db::get_job(&app.db, job_id).await {
        Ok(r) => r,
        Err(e) => {
            return Html(format!("<p>Error loading job: {e}</p>")).into_response();
        }
    };

    // Parse CV JSON (same pattern as job_detail)
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

    // Extract name/title from master CV profile frontmatter
    let (name, title) = match db::get_master_cv(&app.db).await {
        Ok(master) => profile::extract_name_title(&master),
        Err(_) => (String::new(), String::new()),
    };

    CvPrintTemplate {
        name,
        title,
        summary,
        skills,
        experiences,
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
    let review_notes = body.review_notes.as_deref();
    let decision_reason = body.decision_reason.as_deref();

    if approved {
        let _ = db::approve_job(&app.db, job_id, review_notes).await;
    } else {
        let reason = body.reason.as_deref().unwrap_or("").to_string();
        let _ = db::reject_job(&app.db, job_id, &reason, review_notes, decision_reason).await;
    }
    // Redirect back to the job page
    axum::response::Redirect::to(&format!("/jobs/{job_id}")).into_response()
}

#[derive(Deserialize)]
struct RegenerateForm {
    review_notes: Option<String>,
}

async fn regenerate_job(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
    Form(body): Form<RegenerateForm>,
) -> Response {
    let feedback = body.review_notes.as_deref().unwrap_or("");
    let _ = db::save_review_notes(&app.db, job_id, feedback).await;
    if let Err(e) = generate::regenerate_cv(&app, job_id, feedback).await {
        eprintln!("regenerate {job_id} failed: {e}");
        let _ = db::set_status(&app.db, job_id, "failed").await;
    }
    axum::response::Redirect::to(&format!("/jobs/{job_id}")).into_response()
}

// GET /settings — profile hub: CV preview + manual edit.
async fn settings_page(State(app): State<AppState>) -> impl IntoResponse {
    let master_cv = db::get_master_cv(&app.db).await.unwrap_or_default();
    let search_queries = db::list_search_queries(&app.db).await.unwrap_or_default();
    let recent_feedback = db::list_quality_decisions(&app.db, 10).await.unwrap_or_default();
    let scheduler_runs = db::list_scheduler_runs(&app.db, 10).await.unwrap_or_default();
    SettingsTemplate { master_cv, search_queries, recent_feedback, scheduler_runs, status: String::new() }
}

// POST /settings — save manual CV edits to DB.
async fn save_settings(
    State(app): State<AppState>,
    Form(body): Form<SettingsForm>,
) -> Response {
    let _ = db::upsert_master_cv(&app.db, &body.master_cv).await;
    Html("").into_response()
}

// POST /profile/sync — force re-sync profile from files to DB.
async fn profile_sync(State(app): State<AppState>) -> Response {
    match profile::sync_profile_to_db(&app.db).await {
        Ok(()) => Html("<span style=\"color:var(--status-ok)\">✓ Profile synced from files.</span>".to_string()).into_response(),
        Err(e) => Html(format!("<span style=\"color:var(--status-err)\">Sync failed: {e}</span>")).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Search queries CRUD
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SearchQueryForm {
    query: String,
}

async fn render_search_queries_fragment(app: &AppState, status: &str) -> Response {
    let rows = db::list_search_queries(&app.db).await.unwrap_or_default();
    SearchQueriesTemplate { search_queries: rows, status: status.to_string() }.into_response()
}

async fn search_queries_add(
    State(app): State<AppState>,
    Form(body): Form<SearchQueryForm>,
) -> Response {
    let q = body.query.trim().to_lowercase();
    if !q.is_empty() {
        let _ = db::add_search_query(&app.db, &q).await;
    }
    render_search_queries_fragment(&app, "").await
}

async fn search_queries_delete(
    State(app): State<AppState>,
    Path(id): Path<i64>,
) -> Response {
    let _ = db::delete_search_query(&app.db, id).await;
    render_search_queries_fragment(&app, "").await
}

async fn search_queries_regenerate(State(app): State<AppState>) -> Response {
    let started = std::time::Instant::now();
    match generate::generate_search_queries(&app).await {
        Ok(qs) => {
            let count = qs.len();
            let _ = db::replace_search_queries(&app.db, &qs).await;
            let elapsed = started.elapsed().as_secs_f32();
            render_search_queries_fragment(&app, &format!("✓ regenerated {count} queries in {elapsed:.1}s")).await
        }
        Err(e) => {
            render_search_queries_fragment(&app, &format!("regenerate failed: {e}")).await
        }
    }
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

async fn scheduler_run(
    State(app): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Response {
    // Check token
    let token = std::env::var("SCHEDULER_TOKEN").unwrap_or_default();
    if token.is_empty() {
        return (axum::http::StatusCode::UNAUTHORIZED,
                Html("SCHEDULER_TOKEN not configured".to_string())).into_response();
    }
    let auth = headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if auth != format!("Bearer {token}") {
        return (axum::http::StatusCode::UNAUTHORIZED,
                Html("Invalid token".to_string())).into_response();
    }

    // Concurrency guard
    if app.scheduler_running
        .compare_exchange(false, true, std::sync::atomic::Ordering::AcqRel, std::sync::atomic::Ordering::Acquire)
        .is_err()
    {
        return (axum::http::StatusCode::CONFLICT,
                Html("Scheduler run already in progress".to_string())).into_response();
    }

    // Fetch enabled queries
    let queries = match db::list_enabled_search_queries(&app.db).await {
        Ok(qs) => qs,
        Err(e) => {
            app.scheduler_running.store(false, std::sync::atomic::Ordering::Release);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Html(format!("DB error: {e}"))).into_response();
        }
    };

    if queries.is_empty() {
        app.scheduler_running.store(false, std::sync::atomic::Ordering::Release);
        return (axum::http::StatusCode::OK,
                Html("No enabled queries".to_string())).into_response();
    }

    // Create audit row
    let run_id = match db::create_scheduler_run(&app.db, queries.len() as i64).await {
        Ok(id) => id,
        Err(e) => {
            app.scheduler_running.store(false, std::sync::atomic::Ordering::Release);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Html(format!("DB error: {e}"))).into_response();
        }
    };

    // Spawn background task
    let app_bg = app.clone();
    let running_flag = app.scheduler_running.clone();
    let queries_count = queries.len();

    tokio::spawn(async move {
        let _guard = BoolGuard(running_flag.clone());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(36000); // 10h
        let mut errors: Vec<String> = Vec::new();

        for q in &queries {
            if std::time::Instant::now() >= deadline {
                eprintln!("scheduler: time budget exhausted");
                errors.push("time budget exhausted".to_string());
                break;
            }

            let search_id = Uuid::new_v4();
            match crawler::run_search_by_keywords(app_bg.clone(), search_id, &q.query, q.id).await {
                Ok(()) => {
                    eprintln!("scheduler: processed '{}'", q.query);
                }
                Err(e) => {
                    eprintln!("scheduler: '{}' failed: {e}", q.query);
                    errors.push(format!("{}: {e}", q.query));
                }
            }
        }

        // Finish audit row
        let errors_json = if errors.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&errors).unwrap_or_default())
        };
        let _ = db::finish_scheduler_run(&app_bg.db, run_id, errors_json.as_deref()).await;
    });

    (axum::http::StatusCode::ACCEPTED,
     Html(format!("Accepted: {queries_count} queries"))).into_response()
}

/// Drop guard for AtomicBool — releases on panic.
struct BoolGuard(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Drop for BoolGuard {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL missing — .env not loaded?");

    // create_if_missing(true): sqlx defaults this to false, so on a fresh volume
    // (fresh-VM deploy) the db file doesn't exist yet and connect() panics with
    // SQLITE_CANTOPEN. Bare metal always worked only because jobagent.db already
    // existed from M1's setup. This lets the app self-bootstrap its db. (M10 fix.)
    let pool = SqlitePool::connect_with(
        database_url
            .as_str()
            .parse::<SqliteConnectOptions>()
            .expect("bad DATABASE_URL")
            .create_if_missing(true),
    )
    .await
    .expect("failed to connect to SQLite");

    // Run migrations embedded at compile time via `sqlx::migrate!()`.
    // ponytail: this replaces the runtime `sqlx migrate run` CLI invocation —
    // no sqlx-cli binary needed in the container, no MSRV-breakage from
    // `cargo install sqlx-cli` pulling a too-new toolchain (M10 gotcha).
    // The macro reads ./migrations at build time and bakes them into the binary.
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let state = AppState::from_env(pool);

    // Sync profile from files → DB on startup (best-effort; db may be empty on first run).
    if let Err(e) = profile::sync_profile_to_db(&state.db).await {
        eprintln!("profile sync on startup: {e}");
    }

    let app = Router::new()
        .route("/",                  get(index))
        .route("/jobs",              post(submit_job))
        .route("/jobs/search",       post(submit_search))
        .route("/jobs/:id",          get(job_detail))
        .route("/jobs/:id/cv",       get(cv_print))
        .route("/jobs/:id/card",     get(job_card))
        .route("/jobs/:id/decision", post(job_decision))
        .route("/jobs/:id/regenerate", post(regenerate_job))
        .route("/searches/:id/card", get(search_card))
        .route("/settings",          get(settings_page).post(save_settings))
        .route("/profile/sync",      post(profile_sync))
        .route("/settings/searches",            post(search_queries_add))
        .route("/settings/searches/:id",        post(search_queries_delete))
        .route("/settings/searches/regenerate", post(search_queries_regenerate))
        .route("/scheduler/run",                post(scheduler_run))
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
