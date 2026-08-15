// ponytail: thin JSON layer for AI agents and curl-based debugging.
// Every handler is a short wrapper around existing db:: / generate:: / profile:: fns.
// No new business logic, no new deps, no auth (single-user local tool).

use std::sync::Arc;

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
use crate::llm::Provider;
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

#[derive(Deserialize)]
struct ManualJobBody {
    title:       String,
    description: String,
    company:     Option<String>,
    source_url:  Option<String>,
}

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

async fn submit_manual_job_api(
    State(app): State<AppState>,
    Json(body): Json<ManualJobBody>,
) -> impl IntoResponse {
    let title = body.title.trim().to_string();
    let description = body.description.trim().to_string();
    if title.is_empty() || description.is_empty() {
        return err(StatusCode::BAD_REQUEST, "title and description are required").into_response();
    }
    let company = body.company.unwrap_or_default();
    let source_url = body.source_url.unwrap_or_default();

    let job_id = match db::create_manual_job_stub(&app.db, &title, &company, &description, &source_url).await {
        Ok(id) => id,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &format!("create_manual_job_stub: {e}")).into_response(),
    };

    tokio::spawn({
        let app = app.clone();
        async move {
            if let Err(e) = generate::process_manual_job(&app, job_id).await {
                eprintln!("process_manual_job {job_id} failed: {e}");
                let _ = db::delete_job(&app.db, job_id).await;
            }
        }
    });

    Json(json!({"id": job_id.to_string()})).into_response()
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
    let openai_compat = match llm.provider {
        Provider::Openai | Provider::OpenaiCompat => 1,
        _ => 0,
    };
    Json(json!({
        "endpoint": llm.endpoint,
        "api_key": llm.api_key,
        "model": llm.model,
        "provider": llm.provider.to_string(),
        "mock_llm": llm.mock_llm,
        "llm_openai_compat": openai_compat,
    }))
}

#[derive(Deserialize)]
struct LlmConfigBody {
    endpoint: String,
    api_key: String,
    model: String,
    provider: Option<String>,
    mock_llm: Option<bool>,
}

async fn llm_config_save(
    State(app): State<AppState>,
    Json(body): Json<LlmConfigBody>,
) -> impl IntoResponse {
    let provider = body.provider.as_deref()
        .filter(|s| !s.is_empty())
        .map(Provider::parse)
        .unwrap_or_else(|| Provider::from_endpoint(body.endpoint.trim()));
    let config = db::LlmConfigRow {
        endpoint: body.endpoint.trim().to_string(),
        api_key: body.api_key.trim().to_string(),
        model: body.model.trim().to_string(),
        provider,
        mock: body.mock_llm.unwrap_or(false),
    };
    if let Err(e) = db::save_llm_config(&app.db, &config).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response();
    }
    let mut llm = app.llm_config.write().unwrap_or_else(|e| e.into_inner());
    llm.endpoint = config.endpoint;
    llm.api_key = config.api_key;
    llm.model = config.model;
    llm.provider = config.provider;
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
// Agent settings (JSON API)
// ---------------------------------------------------------------------------

async fn agent_settings_get(State(app): State<AppState>) -> impl IntoResponse {
    let agent = db::get_agent_settings(&app.db).await.unwrap_or_default();
    let last_ingest = db::get_wiki_last_ingest_at(&app.db).await.ok().flatten();
    Json(json!({
        "ctx_window": agent.ctx_window,
        "max_output": agent.max_output,
        "thinking_effort": agent.thinking_effort,
        "wiki_query_max_hops": agent.wiki_query_max_hops,
        "wiki_auto_ingest": agent.wiki_auto_ingest,
        "wiki_last_ingest_at": last_ingest,
        "max_review_iterations": agent.max_review_iterations,
    }))
}

#[derive(Deserialize)]
struct AgentSettingsBody {
    ctx_window:              Option<i64>,
    max_output:              Option<i64>,
    thinking_effort:         Option<String>,
    wiki_query_max_hops:     Option<i64>,
    wiki_auto_ingest:        Option<bool>,
    max_review_iterations:   Option<i64>,
}

async fn agent_settings_save(
    State(app): State<AppState>,
    Json(body): Json<AgentSettingsBody>,
) -> impl IntoResponse {
    let current = db::get_agent_settings(&app.db).await.unwrap_or_default();
    let config = db::AgentSettings {
        ctx_window:              body.ctx_window.unwrap_or(current.ctx_window).max(1),
        max_output:              body.max_output.unwrap_or(current.max_output).max(1),
        thinking_effort:         body.thinking_effort.unwrap_or(current.thinking_effort),
        wiki_query_max_hops:     body.wiki_query_max_hops.unwrap_or(current.wiki_query_max_hops).max(1),
        wiki_auto_ingest:        body.wiki_auto_ingest.unwrap_or(current.wiki_auto_ingest),
        max_review_iterations:   body.max_review_iterations.unwrap_or(current.max_review_iterations).max(1),
    };
    if let Err(e) = db::save_agent_settings(&app.db, &config).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response();
    }
    ok().into_response()
}

// ---------------------------------------------------------------------------
// Per-agent overrides (JSON API)
// ---------------------------------------------------------------------------

async fn agent_overrides_get(State(app): State<AppState>) -> impl IntoResponse {
    let overrides = db::get_agent_overrides(&app.db).await.unwrap_or_default();
    let mut map = serde_json::Map::new();
    for role in db::AGENT_ROLES {
        if let Some(o) = overrides.get(*role) {
            let mut obj = serde_json::Map::new();
            if let Some(mo) = o.max_output {
                obj.insert("max_output".into(), json!(mo));
            }
            if let Some(ref te) = o.thinking_effort {
                obj.insert("thinking_effort".into(), json!(te));
            }
            map.insert(role.to_string(), Value::Object(obj));
        } else {
            map.insert(role.to_string(), json!({}));
        }
    }
    Json(Value::Object(map))
}

#[derive(Deserialize)]
struct OverrideEntry {
    max_output:      Option<i64>,
    thinking_effort: Option<String>,
}

async fn agent_overrides_save(
    State(app): State<AppState>,
    Json(body): Json<std::collections::HashMap<String, OverrideEntry>>,
) -> impl IntoResponse {
    let mut entries = Vec::new();
    for role in db::AGENT_ROLES {
        if let Some(entry) = body.get(*role) {
            let max_output = entry.max_output.filter(|_| entry.max_output.is_some());
            let effort = entry.thinking_effort.clone().filter(|s| !s.is_empty() && s != "inherit");
            entries.push((role.to_string(), max_output, effort));
        }
    }
    if let Err(e) = db::save_agent_overrides(&app.db, &entries).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response();
    }
    ok().into_response()
}

// ---------------------------------------------------------------------------
// Pipeline config (GET)
// ---------------------------------------------------------------------------

async fn pipeline_config_get(State(app): State<AppState>) -> impl IntoResponse {
    let c = db::get_pipeline_config(&app.db).await.unwrap_or_default();
    Json(json!({
        "llm_concurrency": c.llm_concurrency,
        "max_jobs_per_crawl": c.max_jobs_per_crawl,
    }))
}

// ---------------------------------------------------------------------------
// Profile lock (GET)
// ---------------------------------------------------------------------------

async fn profile_lock_get(State(app): State<AppState>) -> impl IntoResponse {
    let unlocked = db::get_unlocked_files(&app.db).await.unwrap_or_default();
    Json(json!({
        "unlocked_files": unlocked,
    }))
}

#[derive(Deserialize)]
struct ProfileLockBody {
    unlocked_files: Vec<String>,
}

async fn profile_lock_save(
    State(app): State<AppState>,
    Json(body): Json<ProfileLockBody>,
) -> impl IntoResponse {
    if let Err(e) = db::save_unlocked_files(&app.db, &body.unlocked_files).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response();
    }
    ok().into_response()
}

// ---------------------------------------------------------------------------
// Pipeline config (save)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PipelineBody {
    llm_concurrency:    i64,
    max_jobs_per_crawl: i64,
}

async fn pipeline_config_save(
    State(app): State<AppState>,
    Json(body): Json<PipelineBody>,
) -> impl IntoResponse {
    let cfg = db::PipelineConfig {
        llm_concurrency: body.llm_concurrency.max(1),
        max_jobs_per_crawl: body.max_jobs_per_crawl.max(1),
    };
    if let Err(e) = db::save_pipeline_config(&app.db, &cfg).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response();
    }
    let new_sem = Arc::new(tokio::sync::Semaphore::new(cfg.llm_concurrency as usize));
    let mut guard = app.llm_semaphore.write().unwrap_or_else(|e| e.into_inner());
    *guard = new_sem;
    ok().into_response()
}

// ---------------------------------------------------------------------------
// Test LLM connection
// ---------------------------------------------------------------------------

async fn test_llm(State(app): State<AppState>) -> impl IntoResponse {
    match crate::llm::transport::test_llm_connection(&app).await {
        Ok(latency_ms) => Json(json!({ "ok": true, "latency_ms": latency_ms })).into_response(),
        Err(e) => err(StatusCode::BAD_GATEWAY, &e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Scheduler runs list
// ---------------------------------------------------------------------------

async fn scheduler_runs_list(State(app): State<AppState>) -> impl IntoResponse {
    let runs = db::list_scheduler_runs(&app.db, 10).await.unwrap_or_default();
    Json(json!({
        "runs": runs.iter().map(|r| json!({
            "started_at": r.started_at,
            "finished_at": r.finished_at,
            "status": r.status,
            "queries_run": r.queries_run,
            "jobs_found": r.jobs_found,
            "jobs_filtered": r.jobs_filtered,
        })).collect::<Vec<_>>(),
    }))
}

// ---------------------------------------------------------------------------
// Thinking effort options (static, provider-aware)
// ---------------------------------------------------------------------------

async fn thinking_effort_options() -> impl IntoResponse {
    Json(json!({
        "all": ["none", "minimal", "low", "medium", "high", "xhigh", "adaptive"],
        "providers": {
            "openai": ["none", "minimal", "low", "medium", "high", "xhigh"],
            "anthropic": ["none", "minimal", "low", "medium", "high", "xhigh", "adaptive"],
            "google": ["none", "minimal", "low", "medium", "high", "xhigh", "adaptive"],
            "openai-compat": ["none", "minimal", "low", "medium", "high", "xhigh"],
        }
    }))
}

// ---------------------------------------------------------------------------
// Fetch models list (JSON API)
// ---------------------------------------------------------------------------

async fn fetch_models_list(State(app): State<AppState>) -> impl IntoResponse {
    match crate::llm::fetch_models(&app).await {
        Ok(models) => {
            let items: Vec<Value> = models.iter().map(|m| {
                let mut obj = json!({"id": m.id});
                if let Some(ctx) = m.context_window {
                    obj["context_window"] = json!(ctx);
                }
                obj
            }).collect();
            Json(json!({"models": items})).into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, &e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Fetch model capabilities (JSON API)
// ---------------------------------------------------------------------------

async fn fetch_model_caps(State(app): State<AppState>) -> impl IntoResponse {
    let model = {
        let cfg = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
        cfg.model.clone()
    };
    if model.trim().is_empty() {
        return err(StatusCode::BAD_REQUEST, "no model configured").into_response();
    }
    match crate::llm::fetch_capabilities(&app, &model).await {
        Ok(caps) => Json(json!({
            "model": model,
            "ctx_window": caps.ctx_window,
            "max_output": caps.max_output,
            "source": caps.source,
        })).into_response(),
        Err(e) => err(StatusCode::BAD_GATEWAY, &e.to_string()).into_response(),
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
        .route("/api/jobs/manual",          post(submit_manual_job_api))
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
        .route("/api/settings/agent",       get(agent_settings_get).put(agent_settings_save))
        .route("/api/settings/agent-overrides", get(agent_overrides_get).put(agent_overrides_save))
        .route("/api/settings/pipeline",      get(pipeline_config_get).post(pipeline_config_save))
        .route("/api/settings/profile-lock",  get(profile_lock_get).post(profile_lock_save))
        .route("/api/settings/test-llm",      post(test_llm))
        .route("/api/settings/scheduler-runs", get(scheduler_runs_list))
        .route("/api/settings/thinking-effort-options", get(thinking_effort_options))
        .route("/api/models",               get(fetch_models_list))
        .route("/api/model-capabilities",   get(fetch_model_caps))
        .route("/api/db/query",             get(db_query))
        .with_state(state)
}
