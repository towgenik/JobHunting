use axum::{
    extract::{Form, State},
    response::{Html, IntoResponse, Response},
};
use axum::body::Bytes;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use crate::{AppState, db, events, crawler, templates::{SettingsTemplate, SchedulerRunsTemplate}};
use crate::llm::Provider;
use super::forms::*;
use super::BoolGuard;

// GET /settings — LLM config form + scheduler form + runs history
pub async fn settings_page(State(app): State<AppState>) -> impl IntoResponse {
    let (endpoint, api_key, model, provider, mock_llm) = {
        let llm = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
        (llm.endpoint.clone(), llm.api_key.clone(), llm.model.clone(), llm.provider, llm.mock_llm)
    };
    let sched = db::get_scheduler_config(&app.db).await.unwrap_or(db::SchedulerConfigRow {
        interval_minutes: 0,
        date_range: 1,
        max_pages: 5,
    });
    let runs = db::list_scheduler_runs(&app.db, 10).await.unwrap_or_default();
    let agent = db::get_agent_settings(&app.db).await.unwrap_or_default();
    let pipeline = db::get_pipeline_config(&app.db).await.unwrap_or_default();
    let unlocked = db::get_unlocked_files(&app.db).await.unwrap_or_default();
   let all_files: Vec<String> = crate::profile::list_profile_files()
       .unwrap_or_default()
       .into_iter()
       .map(|f| f.path)
       .collect();
    let overrides = db::get_agent_overrides(&app.db).await.unwrap_or_default();
    let agent_override_rows: Vec<(String, String, String)> =
        db::AGENT_ROLES.iter().map(|role| {
            let o = overrides.get(*role);
            (
                role.to_string(),
                o.and_then(|x| x.max_output).map(|v| v.to_string()).unwrap_or_default(),
                o.and_then(|x| x.thinking_effort.clone()).unwrap_or_default(),
            )
        }).collect();
   SettingsTemplate {
        llm_endpoint: endpoint,
        llm_api_key: api_key,
        llm_model: model,
        llm_provider: provider.to_string(),
        llm_mock: mock_llm,
        scheduler_interval: sched.interval_minutes,
        scheduler_date_range: sched.date_range,
        scheduler_max_pages: sched.max_pages,
        scheduler_runs: runs,
        agent_ctx_window: agent.ctx_window,
        agent_max_output: agent.max_output,
        agent_thinking_effort: agent.thinking_effort,
        agent_wiki_query_max_hops: agent.wiki_query_max_hops,
        wiki_auto_ingest: agent.wiki_auto_ingest,
       agent_max_review_iterations: agent.max_review_iterations,
       llm_concurrency:    pipeline.llm_concurrency,
        agent_override_rows:           agent_override_rows,
       max_jobs_per_crawl: pipeline.max_jobs_per_crawl,
        profile_unlocked_files: unlocked,
        profile_all_files:      all_files,
    }
}

// POST /settings/llm — save LLM config to DB + update Arc<RwLock>
pub async fn settings_llm_save(
    State(app): State<AppState>,
    Form(body): Form<LlmSettingsForm>,
) -> Response {
    let provider = body.provider.as_deref()
        .filter(|s| !s.is_empty())
        .map(Provider::parse)
        .unwrap_or_else(|| Provider::from_endpoint(body.endpoint.trim()));
    let config = db::LlmConfigRow {
        endpoint: body.endpoint.trim().to_string(),
        api_key: body.api_key.trim().to_string(),
        model: body.model.trim().to_string(),
        provider,
        mock: body.mock_llm.as_deref() == Some("on"),
    };
    if let Err(e) = db::save_llm_config(&app.db, &config).await {
        return Html(format!("<span style=\"color:var(--status-err)\">Save failed: {e}</span>")).into_response();
    }
    // Update the in-memory lock atomically (recover if poisoned)
    let mut llm = app.llm_config.write().unwrap_or_else(|e| e.into_inner());
    llm.endpoint = config.endpoint;
    llm.api_key = config.api_key;
    llm.model = config.model;
    llm.provider = config.provider;
    llm.mock_llm = config.mock;
    Html("<span style=\"color:var(--status-ok)\">LLM config saved.</span>").into_response()
}

// POST /settings/scheduler — save scheduler config to DB
pub async fn settings_scheduler_save(
    State(app): State<AppState>,
    Form(body): Form<SchedulerSettingsForm>,
) -> Response {
    let config = db::SchedulerConfigRow {
        interval_minutes: body.interval_minutes,
        date_range: body.date_range,
        max_pages: body.max_pages,
    };
    if let Err(e) = db::save_scheduler_config(&app.db, &config).await {
        return Html(format!("<span style=\"color:var(--status-err)\">Save failed: {}</span>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;"))).into_response();
    }
    Html("<span style=\"color:var(--status-ok)\">Scheduler config saved.</span>").into_response()
}

// POST /settings/agent — save agent settings to DB
pub async fn settings_agent_save(
    State(app): State<AppState>,
    Form(body): Form<AgentSettingsForm>,
) -> Response {
    let config = db::AgentSettings {
        ctx_window:              body.ctx_window.max(1),
        max_output:              body.max_output.max(1),
        thinking_effort:         body.thinking_effort,
        wiki_query_max_hops:     body.wiki_query_max_hops.max(1),
        wiki_auto_ingest:        body.wiki_auto_ingest.as_deref() == Some("on"),
        max_review_iterations:   body.max_review_iterations.max(1),
    };
    if let Err(e) = db::save_agent_settings(&app.db, &config).await {
        return Html(format!("<span style=\"color:var(--status-err)\">Save failed: {}</span>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;"))).into_response();
    }
    Html("<span style=\"color:var(--status-ok)\">Agent settings saved.</span>").into_response()
}

// POST /settings/pipeline — save pipeline tuning config to DB
pub async fn settings_pipeline_save(
    State(app): State<AppState>,
    Form(body): Form<PipelineForm>,
) -> Response {
    let config = db::PipelineConfig {
        llm_concurrency:    body.llm_concurrency.max(1),
        max_jobs_per_crawl: body.max_jobs_per_crawl.max(1),
    };
    if let Err(e) = db::save_pipeline_config(&app.db, &config).await {
        return Html(format!("<span style=\"color:var(--status-err)\">Save failed: {}</span>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;"))).into_response();
    }
    // Hot-swap the semaphore: build a new one and swap it in.
    let new_sem = Arc::new(tokio::sync::Semaphore::new(config.llm_concurrency as usize));
    {
        let mut guard = app.llm_semaphore.write().unwrap_or_else(|e| e.into_inner());
        *guard = new_sem;
    }
    Html("<span style=\"color:var(--status-ok)\">Pipeline config saved. Concurrency applied.</span>").into_response()
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

pub async fn scheduler_run(
    State(app): State<AppState>,
) -> Response {
    if app.scheduler_running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return (axum::http::StatusCode::CONFLICT,
                Html("Scheduler run already in progress".to_string())).into_response();
    }

    let sched = db::get_scheduler_config(&app.db).await.unwrap_or(db::SchedulerConfigRow {
        interval_minutes: 0, date_range: 1, max_pages: 5,
    });
    let date_range = sched.date_range.unsigned_abs() as u32;
    let max_pages = sched.max_pages.unsigned_abs() as u32;

    let run_id = match db::create_scheduler_run(&app.db, 0).await {
        Ok(id) => id,
        Err(e) => {
            app.scheduler_running.store(false, Ordering::Release);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Html(format!("DB error: {e}"))).into_response();
        }
    };

    let app_bg = app.clone();
    let running_flag = app.scheduler_running.clone();
    tokio::spawn(async move {
        let _guard = BoolGuard(running_flag.clone());
        if let Err(e) = crawler::scheduler_browse(app_bg.clone(), date_range, max_pages).await {
            let errors = serde_json::to_string(&[format!("{e}")]).unwrap_or_default();
            let _ = db::finish_scheduler_run(&app_bg.db, run_id, Some(&errors)).await;
            events::publish_scheduler_run_finished(&app_bg, run_id, "error");
            return;
        }
        let _ = db::finish_scheduler_run(&app_bg.db, run_id, None).await;
        events::publish_scheduler_run_finished(&app_bg, run_id, "ok");
    });

    (axum::http::StatusCode::ACCEPTED,
     Html(format!("Accepted: firehose browse (date_range={date_range}, max_pages={max_pages})"))).into_response()
}

// GET /settings/scheduler-runs — returns just the runs table fragment (for SSE refresh)
pub async fn settings_scheduler_runs(State(app): State<AppState>) -> Response {
    let runs = db::list_scheduler_runs(&app.db, 10).await.unwrap_or_default();
    SchedulerRunsTemplate { scheduler_runs: runs }.into_response()
}

// POST /settings/test-llm — test LLM endpoint connectivity
pub async fn settings_test_llm(State(app): State<AppState>) -> Response {
    match crate::llm::transport::test_llm_connection(&app).await {
        Ok(latency_ms) => Html(format!(
            "<span style=\"color:var(--status-ok)\">OK — {}ms latency</span>", latency_ms
        )).into_response(),
        Err(e) => Html(format!(
            "<span style=\"color:var(--status-err)\">Failed: {}</span>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;")
        )).into_response(),
    }
}

// POST /settings/profile-lock — save profile unlocked files list
pub async fn settings_profile_lock_save(
    State(app): State<AppState>,
    Form(body): Form<ProfileLockForm>,
) -> Response {
    let files: Vec<String> = body.unlocked_files
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if let Err(e) = db::save_unlocked_files(&app.db, &files).await {
        return Html(format!("<span style=\"color:var(--status-err)\">Save failed: {}</span>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;"))).into_response();
    }
    Html("<span style=\"color:var(--status-ok)\">Profile lock settings saved.</span>").into_response()
}

// POST /settings/agent-overrides — save per-agent overrides
pub async fn settings_agent_overrides_save(
    State(app): State<AppState>,
    body: Bytes,
) -> Response {
    // ponytail: manual parse — serde_urlencoded rejects repeated keys for Vec<T>.
    let params: Vec<(std::borrow::Cow<'_, str>, std::borrow::Cow<'_, str>)> =
        form_urlencoded::parse(&body).collect();
    let roles: Vec<&str> = params.iter()
        .filter(|(k, _)| k == "role")
        .map(|(_, v)| v.as_ref())
        .collect();
    let max_outputs: Vec<&str> = params.iter()
        .filter(|(k, _)| k == "max_output")
        .map(|(_, v)| v.as_ref())
        .collect();
    let efforts: Vec<&str> = params.iter()
        .filter(|(k, _)| k == "thinking_effort")
        .map(|(_, v)| v.as_ref())
        .collect();

    let mut entries = Vec::new();
    for (i, role) in roles.iter().enumerate() {
        if !db::AGENT_ROLES.contains(role) {
            continue;
        }
        let max_output = max_outputs.get(i)
            .and_then(|s| s.trim().parse::<i64>().ok())
            .filter(|_| max_outputs.get(i).map_or(false, |s| !s.trim().is_empty()));
        let effort = efforts.get(i)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "inherit");
        entries.push((role.to_string(), max_output, effort));
    }
    if let Err(e) = db::save_agent_overrides(&app.db, &entries).await {
        return Html(format!("<span style=\"color:var(--status-err)\">Save failed: {}</span>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;"))).into_response();
    }
    Html("<span style=\"color:var(--status-ok)\">Saved</span>").into_response()
}

// POST /settings/fetch-models — query the provider's model list
pub async fn settings_fetch_models(State(app): State<AppState>) -> Response {
    match crate::llm::fetch_models(&app).await {
        Ok(models) => {
            // Return a compact JSON array; the client turns it into a combobox.
            let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
            Html(format!(
                "<script>window.__models = {};</script>",
                serde_json::to_string(&ids).unwrap_or_default()
            )).into_response()
        }
        Err(e) => Html(format!(
            "<span style=\"color:var(--status-err)\">Failed: {}</span>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;")
        )).into_response(),
    }
}

// POST /settings/fetch-capabilities — look up ctx_window + max_output for current model
pub async fn settings_fetch_capabilities(State(app): State<AppState>) -> Response {
    let model = {
        let cfg = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
        cfg.model.clone()
    };
    if model.trim().is_empty() {
        return Html("<span style=\"color:var(--status-err)\">Set a model first.</span>").into_response();
    }
    match crate::llm::fetch_capabilities(&app, &model).await {
        Ok(caps) => Html(format!(
            "<span id=\"caps-result\" data-ctx=\"{}\" data-out=\"{}\" \
             style=\"color:var(--status-ok)\">ctx={}, out={} (source: {})</span>",
            caps.ctx_window, caps.max_output, caps.ctx_window, caps.max_output, caps.source
        )).into_response(),
        Err(e) => Html(format!(
            "<span style=\"color:var(--status-err)\">Failed: {}</span>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;")
        )).into_response(),
    }
}
