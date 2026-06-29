// ponytail: thin JSON layer for AI agents and curl-based debugging.
// Every handler is a short wrapper around existing db:: / generate:: / profile:: fns.
// No new business logic, no new deps, no auth (single-user local tool).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Column;
use uuid::Uuid;

use crate::{db, generate, profile, AppState};
use crate::handlers::jobs::is_jobstreet_url;

/// Shorthand for a JSON error response.
fn err(code: StatusCode, msg: &str) -> impl IntoResponse {
    (code, Json(json!({"error": msg})))
}

/// Shorthand for a JSON ok response.
fn ok() -> impl IntoResponse {
    Json(json!({"ok": true}))
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

async fn health(State(app): State<AppState>) -> impl IntoResponse {
    let sched = app.scheduler_running.load(std::sync::atomic::Ordering::Relaxed);
    let crawl = app.crawl_activity.read().map(|a| a.active).unwrap_or(false);
    Json(json!({
        "ok": true,
        "version": env!("CARGO_PKG_VERSION"),
        "scheduler_running": sched,
        "crawl_active": crawl,
    }))
}

// ---------------------------------------------------------------------------
// Jobs
// ---------------------------------------------------------------------------

async fn list_jobs(State(app): State<AppState>) -> impl IntoResponse {
    use sqlx::Row;
    let rows = sqlx::query("SELECT id, url, title, status, review_score, company FROM jobs ORDER BY rowid DESC")
        .fetch_all(&app.db)
        .await
        .unwrap_or_default();
    let jobs: Vec<Value> = rows.iter().map(|r| {
        let id: String = r.try_get("id").unwrap_or_default();
        let url: String = r.try_get("url").unwrap_or_default();
        let title: Option<String> = r.try_get("title").ok().flatten();
        let status: String = r.try_get("status").unwrap_or_default();
        let score: Option<i64> = r.try_get("review_score").ok().flatten();
        let company: Option<String> = r.try_get("company").ok().flatten();
        json!({"id": id, "url": url, "title": title.unwrap_or_default(), "status": status, "score": score, "company": company})
    }).collect();
    Json(json!({"jobs": jobs}))
}

async fn get_job_detail(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> impl IntoResponse {
    let rec = match db::get_job(&app.db, job_id).await {
        Ok(r) => r,
        Err(e) => return err(StatusCode::NOT_FOUND, &format!("job not found: {e}")).into_response(),
    };
    let cv: Value = serde_json::from_str(&rec.cv).unwrap_or(Value::Null);
    let verification: Value = rec.verification.as_deref()
        .and_then(|s| serde_json::from_str(s).ok()).unwrap_or(Value::Null);
    let rank: Value = rec.rank.as_deref()
        .and_then(|s| serde_json::from_str(s).ok()).unwrap_or(Value::Null);
    Json(json!({
        "id":              rec.id.to_string(),
        "url":             rec.url,
        "title":           rec.title,
        "description":     rec.description,
        "company":         rec.company,
        "cv":              cv,
        "status":          rec.status,
        "review_score":    rec.review_score,
        "review_feedback": rec.review_feedback,
        "verification":    verification,
        "rank":            rank,
        "review_notes":    rec.review_notes,
        "created_at":      rec.created_at,
        "progress":        rec.progress,
    })).into_response()
}

#[derive(Deserialize)]
struct SubmitJobBody { url: String }

async fn submit_job(
    State(app): State<AppState>,
    Json(body): Json<SubmitJobBody>,
) -> impl IntoResponse {
    if !is_jobstreet_url(&body.url) {
        return err(StatusCode::BAD_REQUEST, "Only id.jobstreet.com URLs are supported.").into_response();
    }

    if let Ok(Some(existing_id)) = db::get_job_id_by_url(&app.db, &body.url).await {
        return Json(json!({"id": existing_id.to_string(), "existing": true})).into_response();
    }

    let job_id = match db::create_job_stub(&app.db, &body.url).await {
        Ok(id) => id,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &format!("create_job_stub: {e}")).into_response(),
    };

    tokio::spawn({
        let app = app.clone();
        async move {
            if let Err(e) = generate::process_job(&app, job_id).await {
                eprintln!("process_job {job_id} failed: {e}");
                let _ = db::delete_job(&app.db, job_id).await;
            }
        }
    });

    Json(json!({"id": job_id.to_string(), "existing": false})).into_response()
}

async fn job_card(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> impl IntoResponse {
    let (status, progress) = {
        use sqlx::Row;
        let row = sqlx::query("SELECT status, progress FROM jobs WHERE id = ?")
            .bind(job_id.to_string())
            .fetch_optional(&app.db)
            .await
            .ok()
            .flatten();
        let status = row.as_ref().and_then(|r| r.try_get::<Option<String>, _>("status").ok().flatten()).unwrap_or_default();
        let progress = row.as_ref().and_then(|r| r.try_get::<Option<String>, _>("progress").ok().flatten()).unwrap_or_default();
        (status, progress)
    };
    Json(json!({"id": job_id.to_string(), "status": status, "progress": progress}))
}

#[derive(Deserialize)]
struct RegenerateBody { feedback: Option<String> }

async fn regenerate_job(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
    Json(body): Json<RegenerateBody>,
) -> impl IntoResponse {
    let feedback = body.feedback.as_deref().unwrap_or("");
    let _ = db::save_review_notes(&app.db, job_id, feedback).await;
    if let Err(e) = generate::regenerate_cv(&app, job_id, feedback).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &format!("regenerate: {e}")).into_response();
    }
    ok()    .into_response()
}

/// Batch regenerate: re-run the full pipeline for selected jobs.
/// This re-evaluates fit via pre-screen and regenerates CVs if they pass.
async fn regenerate_batch(
    State(app): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let ids: Vec<String> = body["ids"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    for id_str in &ids {
        if let Ok(id) = uuid::Uuid::parse_str(id_str) {
            let app = app.clone();
            tokio::spawn(async move {
                if let Err(e) = crate::generate::process_job(&app, id).await {
                    eprintln!("api regenerate_batch {id} failed: {e}");
                }
            });
        }
    }
    Json(json!({"ok": true, "count": ids.len()}))
}

async fn delete_job(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> impl IntoResponse {
    match db::delete_job(&app.db, job_id).await {
        Ok(true) => ok().into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "job not found").into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn delete_batch(
    State(app): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let ids: Vec<String> = body["ids"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    match db::delete_jobs(&app.db, &ids).await {
        Ok(n) => Json(json!({"deleted": n})).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Profile
// ---------------------------------------------------------------------------

async fn profile_get(Query(params): Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let files = profile::list_profile_files().unwrap_or_default();
    let current = params.get("file").cloned().unwrap_or_else(|| "index.md".into());
    let content = profile::read_profile_file(&current).unwrap_or_default();
    Json(json!({
        "files": files.iter().map(|f| json!({"path": f.path, "name": f.name})).collect::<Vec<_>>(),
        "current": current,
        "content": content,
    }))
}

#[derive(Deserialize)]
struct ProfileBody { file: String, content: String }

async fn profile_save(
    State(app): State<AppState>,
    Json(body): Json<ProfileBody>,
) -> impl IntoResponse {
    let file = body.file.trim().to_string();
    if file.is_empty() || file.contains("..") {
        return err(StatusCode::BAD_REQUEST, "invalid file path").into_response();
    }
    match profile::write_profile_file(&file, &body.content) {
        Ok(()) => {
            if file == "index.md" {
                let _ = profile::sync_profile_to_db(&app.db).await;
            }
            ok().into_response()
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn profile_sync(State(app): State<AppState>) -> impl IntoResponse {
    match profile::sync_profile_to_db(&app.db).await {
        Ok(()) => ok().into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

async fn llm_config_get(State(app): State<AppState>) -> impl IntoResponse {
    let llm = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
    Json(json!({
        "endpoint": llm.endpoint,
        "model": llm.model,
        "openai_compat": llm.openai_compat,
        "mock_llm": llm.mock_llm,
        // mask api_key — show last 4 chars only
        "api_key_suffix": if llm.api_key.len() > 4 {
            format!("...{}", &llm.api_key[llm.api_key.len()-4..])
        } else { "(empty)".into() },
    }))
}

#[derive(Deserialize)]
struct LlmConfigBody {
    endpoint: String,
    api_key: String,
    model: String,
    openai_compat: Option<bool>,
    mock_llm: Option<bool>,
}

async fn llm_config_save(
    State(app): State<AppState>,
    Json(body): Json<LlmConfigBody>,
) -> impl IntoResponse {
    let config = db::LlmConfigRow {
        endpoint: body.endpoint.trim().to_string(),
        api_key: body.api_key.trim().to_string(),
        model: body.model.trim().to_string(),
        openai_compat: body.openai_compat.unwrap_or(true),
        mock: body.mock_llm.unwrap_or(false),
    };
    if let Err(e) = db::save_llm_config(&app.db, &config).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response();
    }
    let mut llm = app.llm_config.write().unwrap_or_else(|e| e.into_inner());
    llm.endpoint = config.endpoint;
    llm.api_key = config.api_key;
    llm.model = config.model;
    llm.openai_compat = config.openai_compat;
    llm.mock_llm = config.mock;
    ok().into_response()
}

async fn scheduler_config_get(State(app): State<AppState>) -> impl IntoResponse {
    let c = db::get_scheduler_config(&app.db).await.unwrap_or(db::SchedulerConfigRow {
        interval_minutes: 0, date_range: 1, max_pages: 5,
    });
    Json(json!({
        "interval_minutes": c.interval_minutes,
        "date_range": c.date_range,
        "max_pages": c.max_pages,
    }))
}

#[derive(Deserialize)]
struct SchedulerConfigBody {
    interval_minutes: i64,
    date_range: i64,
    max_pages: i64,
}

async fn scheduler_config_save(
    State(app): State<AppState>,
    Json(body): Json<SchedulerConfigBody>,
) -> impl IntoResponse {
    if let Err(e) = db::save_scheduler_config(&app.db, &db::SchedulerConfigRow {
        interval_minutes: body.interval_minutes,
        date_range: body.date_range,
        max_pages: body.max_pages,
    }).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response();
    }
    ok().into_response()
}

// ---------------------------------------------------------------------------
// Crawl
// ---------------------------------------------------------------------------

async fn crawl_status(State(app): State<AppState>) -> impl IntoResponse {
    let activity = app.crawl_activity.read().map(|a| a.clone()).unwrap_or_default();
    let (terminal, total) = match activity.search_id {
        Some(sid) if activity.active => db::get_search_progress(&app.db, sid).await.unwrap_or((0, 0)),
        _ => (0, 0),
    };
    Json(json!({
        "active": activity.active,
        "stopping": activity.stopping,
        "message": activity.message,
        "terminal": terminal,
        "total": total,
    }))
}

async fn crawl_stop(State(app): State<AppState>) -> impl IntoResponse {
    app.crawl_cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut a) = app.crawl_activity.write() {
        a.stopping = true;
    }
    ok().into_response()
}

// ---------------------------------------------------------------------------
// Wiki operations
// ---------------------------------------------------------------------------

async fn wiki_ingest(State(app): State<AppState>) -> impl IntoResponse {
    let dir = crate::profile::profile_dir();
    match crate::wiki::ingest(&app, &dir).await {
        Ok(report) => Json(json!({
            "ok": true,
            "sources_processed": report.sources_processed,
            "nodes_created": report.nodes_created,
            "nodes_appended": report.nodes_appended,
            "errors": report.errors,
        })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn wiki_lint(State(_app): State<AppState>) -> impl IntoResponse {
    let dir = crate::profile::profile_dir();
    match crate::wiki::lint(&dir).await {
        Ok(()) => ok().into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn wiki_lint_report() -> impl IntoResponse {
    let dir = crate::profile::profile_dir();
    match crate::wiki::read_lint_report(&dir) {
        Ok(report) => Json(json!({"ok": true, "report": report})).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

async fn scheduler_run(State(app): State<AppState>) -> impl IntoResponse {
    if app.scheduler_running
        .compare_exchange(false, true, std::sync::atomic::Ordering::AcqRel, std::sync::atomic::Ordering::Acquire)
        .is_err()
    {
        return err(StatusCode::CONFLICT, "scheduler already running").into_response();
    }

    let sched = db::get_scheduler_config(&app.db).await.unwrap_or(db::SchedulerConfigRow {
        interval_minutes: 0, date_range: 1, max_pages: 5,
    });
    let dr = sched.date_range.unsigned_abs() as u32;
    let mp = sched.max_pages.unsigned_abs() as u32;

    let run_id = match db::create_scheduler_run(&app.db, 0).await {
        Ok(id) => id,
        Err(e) => {
            app.scheduler_running.store(false, std::sync::atomic::Ordering::Release);
            return err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")).into_response();
        }
    };

    let app_bg = app.clone();
    let running = app.scheduler_running.clone();
    tokio::spawn(async move {
        let _guard = crate::handlers::BoolGuard(running);
        if let Err(e) = crate::crawler::scheduler_browse(app_bg.clone(), dr, mp).await {
            let errors = serde_json::to_string(&[format!("{e}")]).unwrap_or_default();
            let _ = db::finish_scheduler_run(&app_bg.db, run_id, Some(&errors)).await;
            return;
        }
        let _ = db::finish_scheduler_run(&app_bg.db, run_id, None).await;
    });

    (StatusCode::ACCEPTED, Json(json!({"accepted": true, "run_id": run_id}))).into_response()
}

// ---------------------------------------------------------------------------
// Debug: arbitrary SELECT query
// ---------------------------------------------------------------------------

async fn db_query(
    State(app): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let q = match params.get("q") {
        Some(q) => q.trim(),
        None => return err(StatusCode::BAD_REQUEST, "missing ?q= parameter").into_response(),
    };
    // ponytail: case-insensitive starts_with check; reject anything that isn't SELECT
    let upper = q.to_uppercase();
    if !upper.starts_with("SELECT") && !upper.starts_with("WITH") {
        return err(StatusCode::FORBIDDEN, "only SELECT/WITH queries allowed").into_response();
    }

    // Use raw sqlx to execute and return columns + rows as JSON
    match sqlx::query(q).fetch_all(&app.db).await {
        Ok(rows) => {
            let result: Vec<Value> = rows.iter().map(|row| {
                use sqlx::Row;
                let mut map = serde_json::Map::new();
                // ponytail: iterate columns — sqlite returns column metadata via row
                for i in 0..row.len() {
                    let name = row.column(i).name().to_string();
                    // Try types in order of likelihood: string, i64, f64, bool, null
                    let val: Value = if let Ok(v) = row.try_get::<String, _>(i) {
                        json!(v)
                    } else if let Ok(v) = row.try_get::<i64, _>(i) {
                        json!(v)
                    } else if let Ok(v) = row.try_get::<f64, _>(i) {
                        json!(v)
                    } else if let Ok(v) = row.try_get::<bool, _>(i) {
                        json!(v)
                    } else {
                        Value::Null
                    };
                    map.insert(name, val);
                }
                Value::Object(map)
            }).collect();
            Json(json!({"columns": rows.first().map(|r| { use sqlx::Row; r.len() }).unwrap_or(0), "rows": result})).into_response()
        }
        Err(e) => err(StatusCode::BAD_REQUEST, &e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the /api router. Prefix all paths here so it merges cleanly
/// into the main Router without .nest() state gymnastics.
pub fn api_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/health",               get(health))
        .route("/api/jobs",                 get(list_jobs).post(submit_job))
        .route("/api/jobs/:id",             get(get_job_detail).delete(delete_job))
        .route("/api/jobs/:id/card",        get(job_card))
        .route("/api/jobs/:id/regenerate",  post(regenerate_job))
        .route("/api/jobs/delete-batch",    post(delete_batch))
        .route("/api/jobs/regenerate-batch", post(regenerate_batch))
        .route("/api/profile",              get(profile_get).post(profile_save))
        .route("/api/profile/sync",         post(profile_sync))
        .route("/api/settings/llm",         get(llm_config_get).post(llm_config_save))
        .route("/api/settings/scheduler",   get(scheduler_config_get).post(scheduler_config_save))
        .route("/api/scheduler/run",        post(scheduler_run))
        .route("/api/crawl/status",         get(crawl_status))
        .route("/api/crawl/stop",           post(crawl_stop))
        .route("/api/wiki/ingest",          post(wiki_ingest))
        .route("/api/wiki/lint",            post(wiki_lint))
        .route("/api/wiki/lint-report",     get(wiki_lint_report))
        .route("/api/db/query",             get(db_query))
        .with_state(state)
}
