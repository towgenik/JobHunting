use axum::{
    extract::{Form, State},
    response::{Html, IntoResponse, Response},
};
use std::sync::atomic::Ordering;
use crate::{AppState, db, events, crawler, templates::{SettingsTemplate, SchedulerRunsTemplate}};
use super::forms::*;
use super::BoolGuard;

// GET /settings — LLM config form + scheduler form + runs history
pub async fn settings_page(State(app): State<AppState>) -> impl IntoResponse {
    let (endpoint, api_key, model, openai_compat, mock_llm) = {
        let llm = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
        (llm.endpoint.clone(), llm.api_key.clone(), llm.model.clone(), llm.openai_compat, llm.mock_llm)
    };
    let sched = db::get_scheduler_config(&app.db).await.unwrap_or(db::SchedulerConfigRow {
        interval_minutes: 0,
        date_range: 1,
        max_pages: 5,
    });
    let runs = db::list_scheduler_runs(&app.db, 10).await.unwrap_or_default();
    let agent = db::get_agent_settings(&app.db).await.unwrap_or_default();
    let pipeline = db::get_pipeline_config(&app.db).await.unwrap_or_default();
    SettingsTemplate {
        llm_endpoint: endpoint,
        llm_api_key: api_key,
        llm_model: model,
        llm_openai_compat: openai_compat,
        llm_mock: mock_llm,
        scheduler_interval: sched.interval_minutes,
        scheduler_date_range: sched.date_range,
        scheduler_max_pages: sched.max_pages,
        scheduler_runs: runs,
        status: String::new(),
        agent_ctx_window: agent.ctx_window,
        agent_max_output: agent.max_output,
        agent_thinking_effort: agent.thinking_effort,
        agent_wiki_query_max_hops: agent.wiki_query_max_hops,
        wiki_auto_ingest: agent.wiki_auto_ingest,
        llm_concurrency:    pipeline.llm_concurrency,
        max_jobs_per_crawl: pipeline.max_jobs_per_crawl,
    }
}

// POST /settings/llm — save LLM config to DB + update Arc<RwLock>
pub async fn settings_llm_save(
    State(app): State<AppState>,
    Form(body): Form<LlmSettingsForm>,
) -> Response {
    let config = db::LlmConfigRow {
        endpoint: body.endpoint.trim().to_string(),
        api_key: body.api_key.trim().to_string(),
        model: body.model.trim().to_string(),
        openai_compat: body.openai_compat.as_deref() == Some("on"),
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
    llm.openai_compat = config.openai_compat;
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
        ctx_window:          body.ctx_window.max(1000),
        max_output:          body.max_output.clamp(256, 65536),
        thinking_effort:     body.thinking_effort,
        wiki_query_max_hops: body.wiki_query_max_hops.clamp(1, 50),
        wiki_auto_ingest:    body.wiki_auto_ingest.as_deref() == Some("on"),
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
        llm_concurrency:    body.llm_concurrency.clamp(1, 64),
        max_jobs_per_crawl: body.max_jobs_per_crawl.clamp(5, 500),
    };
    if let Err(e) = db::save_pipeline_config(&app.db, &config).await {
        return Html(format!("<span style=\"color:var(--status-err)\">Save failed: {}</span>",
            e.to_string().replace('&', "&amp;").replace('<', "&lt;"))).into_response();
    }
    Html("<span style=\"color:var(--status-ok)\">Pipeline config saved. Restart to apply concurrency change.</span>").into_response()
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
