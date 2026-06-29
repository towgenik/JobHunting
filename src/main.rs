mod api;
mod crawler;
mod db;
mod events;
mod generate;
mod handlers;
mod llm;
mod pipeline;
mod profile;
mod templates;
mod wiki;

use axum::{
    routing::{delete, get, post},
    Router,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, RwLock};
use uuid::Uuid;
use crate::handlers::BoolGuard;

// ponytail: SSE helpers — tokio-stream wraps broadcast::Receiver into a Stream
// See src/events.rs for publish_job_update and sse_events.

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct LlmConfig {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub openai_compat: bool,
    pub mock_llm: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub db:               SqlitePool,
    pub http:             reqwest::Client,
    pub llm_config:       Arc<RwLock<LlmConfig>>,
    pub llm_semaphore:    Arc<tokio::sync::Semaphore>,
    pub scheduler_running: Arc<AtomicBool>,
    pub last_scheduler_run: Arc<AtomicI64>,
    pub profile_title_blacklist: Vec<String>,
    pub profile_deal_breaker_keywords: Vec<String>,
    // ponytail: single-user app — one crawl at a time is the actual model, so a
    // global cancel flag + activity slot is enough. Per-search maps would be
    // over-engineering. Upgrade path: HashMap<Uuid, Arc<AtomicBool>> if concurrent
    // crawls ever land.
    pub crawl_cancel:  Arc<AtomicBool>,
    pub crawl_activity: Arc<RwLock<CrawlActivity>>,
    /// SSE event bus for live status updates. Clients subscribe to /events.
    /// Messages are JSON strings: {"id":"<uuid>","status":"...","progress":"..."}
    pub event_bus:     tokio::sync::broadcast::Sender<String>,
    /// In-memory wiki graph for query agent traversal. Shared with ingest tasks.
    pub wiki:          Arc<RwLock<Option<wiki::WikiGraph>>>,
}

/// Live crawl state surfaced to the UI. `search_id` ties the message to the
/// active batch so stale cards can distinguish "this one is done" from "this
/// one is still running".
#[derive(Clone, Debug, Default)]
pub struct CrawlActivity {
    pub search_id: Option<Uuid>,
    pub message:   String,
    pub stopping:  bool,
    pub active:    bool,
}

/// Update the global crawl activity slot.
pub fn set_crawl_activity(app: &AppState, search_id: Option<Uuid>, message: &str) {
    if let Ok(mut a) = app.crawl_activity.write() {
        a.search_id = search_id;
        a.message = message.to_string();
    }
}

/// Mark the crawl as finished — keep the message visible so the user sees the
/// final state, but `active=false` lets the panel drop the Stop button and
/// eventually fade to idle on the next refresh.
pub fn finish_crawl_activity(app: &AppState) {
    if let Ok(mut a) = app.crawl_activity.write() {
        a.active = false;
        a.stopping = false;
    }
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

    let map = profile::parse_frontmatter(&content);

    let parse_csv = |s: &str| -> Vec<String> {
        s.split(',').map(|item| item.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|s| !s.is_empty()).collect()
    };

    let title_blacklist = map.get("title_blacklist")
        .map(|v| parse_csv(v))
        .unwrap_or(default_blacklist);
    let deal_breaker_keywords = map.get("deal_breaker_keywords")
        .map(|v| parse_csv(v))
        .unwrap_or(default_deal_breakers);

    (title_blacklist, deal_breaker_keywords)
}

/// Load LlmConfig from DB first. If any field is empty, fall back to env var.
async fn load_llm_config_with_env_fallback(pool: &SqlitePool) -> LlmConfig {
    let db_config = db::get_llm_config(pool).await.unwrap_or_default();
    let env_endpoint = std::env::var("LLM_ENDPOINT").unwrap_or_default();
    let env_api_key = std::env::var("LLM_API_KEY").unwrap_or_default();
    let env_model = std::env::var("LLM_MODEL").unwrap_or_default();
    let env_mock = std::env::var("LLM_MOCK").is_ok();
    let env_compat = std::env::var("LLM_PROVIDER")
        .map(|v| v.to_lowercase() != "anthropic")
        .unwrap_or_else(|_| !env_endpoint.contains("anthropic.com"));

    let is_empty = db_config.endpoint.is_empty();
    LlmConfig {
        endpoint: if is_empty { env_endpoint } else { db_config.endpoint },
        api_key:  if is_empty { env_api_key } else { db_config.api_key },
        model:    if is_empty { env_model } else { db_config.model },
        openai_compat: if is_empty { env_compat } else { db_config.openai_compat },
        mock_llm: if is_empty { env_mock } else { db_config.mock },
    }
}

impl AppState {
    pub fn new(db: SqlitePool, llm_config: LlmConfig) -> Self {
        let concurrency: usize = std::env::var("LLM_CONCURRENCY")
            .ok().and_then(|s| s.parse().ok()).unwrap_or(2);
        let (profile_title_blacklist, profile_deal_breaker_keywords) =
            parse_profile_frontmatter();

        Self {
            db,
            // ponytail: 180s total timeout. Without this, a hung LLM endpoint
            // holds the semaphore permit forever → after 2 hangs, all future LLM
            // calls block → app deadlocked. Timeout releases permit on error.
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(180))
                .build()
                .expect("failed to build reqwest client"),
            llm_config: Arc::new(RwLock::new(llm_config)),
            llm_semaphore: Arc::new(tokio::sync::Semaphore::new(concurrency)),
            scheduler_running: Arc::new(AtomicBool::new(false)),
            last_scheduler_run: Arc::new(AtomicI64::new(0)),
            profile_title_blacklist,
            profile_deal_breaker_keywords,
            crawl_cancel: Arc::new(AtomicBool::new(false)),
            crawl_activity: Arc::new(RwLock::new(CrawlActivity::default())),
            event_bus: tokio::sync::broadcast::channel::<String>(256).0,
            wiki: Arc::new(RwLock::new(None)),
        }
    }
}

// ---------------------------------------------------------------------------
// Forms
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL missing — .env not loaded?");

    let pool = SqlitePool::connect_with(
        database_url
            .as_str()
            .parse::<SqliteConnectOptions>()
            .expect("bad DATABASE_URL")
            .create_if_missing(true),
    )
    .await
    .expect("failed to connect to SQLite");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    // Recover orphaned jobs: anything in a transient state was mid-flight when
    // the previous process died. Reset to 'failed' so the user can regenerate.
    // ponytail: 'new' and 'pre_screening' included — create_job_stub or
    // pre_screen may have started but process_job never finished.
    match sqlx::query("UPDATE jobs SET status='failed' WHERE status IN ('new','scraping','generating','pre_screening')")
        .execute(&pool).await
    {
        Ok(r) if r.rows_affected() > 0 => {
            eprintln!("recovered {} orphaned job(s) stuck in a transient state", r.rows_affected());
        }
        Err(e) => eprintln!("startup recovery query failed: {e}"),
        _ => {}
    }

    let llm_config = load_llm_config_with_env_fallback(&pool).await;
    let state = AppState::new(pool, llm_config);

    // Sync profile from files → DB on startup
    if let Err(e) = profile::sync_profile_to_db(&state.db).await {
        eprintln!("profile sync on startup: {e}");
    }

    // Load wiki graph from profile directory
    let profile_dir = std::path::PathBuf::from(
        std::env::var("PROFILE_DIR").unwrap_or_else(|_| "./profile".into())
    );
    let wiki_graph = match wiki::WikiGraph::load(&profile_dir) {
        Ok(g) if !g.is_empty() => {
            eprintln!("wiki graph loaded: {} nodes", g.len());
            Some(g)
        }
        Ok(_) => { eprintln!("wiki graph: no nodes found"); None }
        Err(e) => { eprintln!("wiki graph load failed: {e}"); None }
    };
    let state_with_wiki = state.clone();
    *state_with_wiki.wiki.write().unwrap_or_else(|e| e.into_inner()) = wiki_graph;

    // Auto-ingest on startup if enabled and raw/ has newer files
    if db::get_agent_settings(&state_with_wiki.db).await.map(|a| a.wiki_auto_ingest).unwrap_or(false) {
        let last_at = db::get_wiki_last_ingest_at(&state_with_wiki.db).await.ok().flatten();
        if wiki::needs_ingest(&profile_dir, last_at) {
            eprintln!("auto-ingest: raw/ has new files, starting ingest…");
            let app_ingest = state_with_wiki.clone();
            let dir_ingest = profile_dir.clone();
            let wiki_arc = state_with_wiki.wiki.clone();
            tokio::spawn(async move {
                match wiki::ingest(&app_ingest, &dir_ingest).await {
                    Ok(report) => eprintln!("auto-ingest: {}", report.summary()),
                    Err(e) => eprintln!("auto-ingest failed: {e}"),
                }
                // Refresh wiki graph after ingest — write through to shared state
                if let Ok(new_graph) = wiki::WikiGraph::load(&dir_ingest) {
                    let node_count = new_graph.len();
                    *wiki_arc.write().unwrap_or_else(|e| e.into_inner()) = Some(new_graph);
                    eprintln!("auto-ingest: refreshed wiki graph ({node_count} nodes)");
                }
            });
        }
    }

    // Background scheduler task — checks every 60s
    let db_bg = state_with_wiki.db.clone();
    let app_bg = state_with_wiki.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let config = match db::get_scheduler_config(&db_bg).await {
                Ok(c) => c,
                Err(e) => { eprintln!("bg scheduler: db error: {e}"); continue; }
            };
            if config.interval_minutes == 0 { continue; }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_secs() as i64;
            let last = app_bg.last_scheduler_run.load(Ordering::Relaxed);
            if now - last < config.interval_minutes * 60 { continue; }

            if app_bg.scheduler_running.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
                continue;
            }
            app_bg.last_scheduler_run.store(now, Ordering::Relaxed);

            let app_s = app_bg.clone();
            let running = app_bg.scheduler_running.clone();
            let dr = config.date_range.unsigned_abs() as u32;
            let mp = config.max_pages.unsigned_abs() as u32;
            tokio::spawn(async move {
                let _guard = BoolGuard(running);
                if let Err(e) = crawler::scheduler_browse(app_s, dr, mp).await {
                    eprintln!("bg scheduler: browse failed: {e}");
                }
            });
        }
    });

    let app = Router::new()
        .route("/",                  get(handlers::jobs::index))
        .route("/jobs",              post(handlers::jobs::submit_job))
        .route("/jobs/list",         get(handlers::jobs::job_list))
        .route("/jobs/:id",          get(handlers::jobs::job_detail))
        .route("/jobs/:id/cv",       get(handlers::jobs::cv_print))
        .route("/jobs/:id/card",     get(handlers::jobs::job_card))
        .route("/jobs/:id/regenerate", post(handlers::jobs::regenerate_job))
        .route("/jobs/:id/delete", delete(handlers::jobs::delete_job))
        .route("/jobs/delete-batch",     post(handlers::jobs::delete_batch))
        .route("/jobs/regenerate-batch", post(handlers::jobs::regenerate_batch))
        .route("/events",            get(events::sse_events))
        .route("/crawl/status",      get(handlers::jobs::crawl_status))
        .route("/crawl/stop",        post(handlers::jobs::crawl_stop))
        .route("/profile",           get(handlers::profile::profile_page).post(handlers::profile::profile_save))
        .route("/profile/print",     get(handlers::profile_print::profile_print))
        .route("/profile/sync",      post(handlers::profile::profile_sync))
        .route("/wiki/ingest",       post(handlers::wiki::wiki_ingest))
        .route("/wiki/lint",         post(handlers::wiki::wiki_lint))
        .route("/wiki/lint-report",  get(handlers::wiki::wiki_lint_report))
        .route("/settings",          get(handlers::settings::settings_page))
        .route("/settings/llm",      post(handlers::settings::settings_llm_save))
        .route("/settings/scheduler", post(handlers::settings::settings_scheduler_save))
        .route("/settings/agent",     post(handlers::settings::settings_agent_save))
        .route("/scheduler/run",     post(handlers::settings::scheduler_run))
        .merge(api::api_router(state_with_wiki.clone()))
        .with_state(state_with_wiki);

    let bind_addr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".into());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("failed to bind");
    println!("Listening on http://{bind_addr}");
    axum::serve(listener, app).await.expect("server error");
}
