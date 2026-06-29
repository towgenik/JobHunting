use anyhow::Result;
use sqlx::SqlitePool;

// ---------------------------------------------------------------------------
// LLM config in settings table
// ---------------------------------------------------------------------------

pub struct LlmConfigRow {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub openai_compat: bool,
    pub mock: bool,
}

impl Default for LlmConfigRow {
    fn default() -> Self {
        Self { endpoint: String::new(), api_key: String::new(), model: String::new(), openai_compat: true, mock: false }
    }
}

pub async fn get_llm_config(pool: &SqlitePool) -> Result<LlmConfigRow> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT llm_endpoint, llm_api_key, llm_model, llm_openai_compat, llm_mock FROM settings WHERE id = 1"
    )
    .fetch_one(pool)
    .await?;
    Ok(LlmConfigRow {
        endpoint: row.try_get::<Option<String>, _>("llm_endpoint")?.unwrap_or_default(),
        api_key: row.try_get::<Option<String>, _>("llm_api_key")?.unwrap_or_default(),
        model: row.try_get::<Option<String>, _>("llm_model")?.unwrap_or_default(),
        openai_compat: row.try_get::<i64, _>("llm_openai_compat")? != 0,
        mock: row.try_get::<i64, _>("llm_mock")? != 0,
    })
}

pub async fn save_llm_config(pool: &SqlitePool, config: &LlmConfigRow) -> Result<()> {
    sqlx::query(
        "UPDATE settings SET llm_endpoint = ?, llm_api_key = ?, llm_model = ?, llm_openai_compat = ?, llm_mock = ? WHERE id = 1"
    )
    .bind(&config.endpoint)
    .bind(&config.api_key)
    .bind(&config.model)
    .bind(config.openai_compat as i64)
    .bind(config.mock as i64)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Agent settings in settings table
// ---------------------------------------------------------------------------

pub struct AgentSettings {
    pub ctx_window:          i64,
    pub max_output:          i64,
    pub thinking_effort:     String,
    pub wiki_query_max_hops: i64,
    pub wiki_auto_ingest:    bool,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            ctx_window:          1_048_576,
            max_output:          131_072,
            thinking_effort:     "high".into(),
            wiki_query_max_hops: 10,
            wiki_auto_ingest:    false,
        }
    }
}

pub async fn get_agent_settings(pool: &SqlitePool) -> Result<AgentSettings> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT agent_ctx_window, agent_max_output, agent_thinking_effort,
                agent_wiki_query_max_hops, wiki_auto_ingest
         FROM settings WHERE id = 1"
    )
    .fetch_one(pool)
    .await?;
    Ok(AgentSettings {
        ctx_window:          row.try_get::<i64, _>("agent_ctx_window").unwrap_or(200_000),
        max_output:          row.try_get::<i64, _>("agent_max_output").unwrap_or(16384),
        thinking_effort:     row.try_get::<Option<String>, _>("agent_thinking_effort")?.unwrap_or_else(|| "high".into()),
        wiki_query_max_hops: row.try_get::<i64, _>("agent_wiki_query_max_hops").unwrap_or(10),
        wiki_auto_ingest:    row.try_get::<i64, _>("wiki_auto_ingest").unwrap_or(0) != 0,
    })
}

pub async fn save_agent_settings(pool: &SqlitePool, s: &AgentSettings) -> Result<()> {
    sqlx::query(
        "UPDATE settings SET agent_ctx_window = ?, agent_max_output = ?,
         agent_thinking_effort = ?, agent_wiki_query_max_hops = ?,
         wiki_auto_ingest = ? WHERE id = 1"
    )
    .bind(s.ctx_window)
    .bind(s.max_output)
    .bind(&s.thinking_effort)
    .bind(s.wiki_query_max_hops)
    .bind(s.wiki_auto_ingest as i64)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Pipeline tuning (concurrency, per-crawl caps)
// ---------------------------------------------------------------------------

pub struct PipelineConfig {
    pub llm_concurrency:     i64,
    pub max_jobs_per_crawl:  i64,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self { llm_concurrency: 2, max_jobs_per_crawl: 30 }
    }
}

pub async fn get_pipeline_config(pool: &SqlitePool) -> Result<PipelineConfig> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT llm_concurrency, max_jobs_per_crawl FROM settings WHERE id = 1"
    )
    .fetch_one(pool)
    .await?;
    Ok(PipelineConfig {
        llm_concurrency:    row.try_get::<i64, _>("llm_concurrency").unwrap_or(2),
        max_jobs_per_crawl: row.try_get::<i64, _>("max_jobs_per_crawl").unwrap_or(30),
    })
}

pub async fn save_pipeline_config(pool: &SqlitePool, c: &PipelineConfig) -> Result<()> {
    sqlx::query(
        "UPDATE settings SET llm_concurrency = ?, max_jobs_per_crawl = ? WHERE id = 1"
    )
    .bind(c.llm_concurrency)
    .bind(c.max_jobs_per_crawl)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_wiki_last_ingest_at(pool: &SqlitePool) -> Result<Option<i64>> {
    use sqlx::Row;
    let row = sqlx::query("SELECT wiki_last_ingest_at FROM settings WHERE id = 1")
        .fetch_one(pool)
        .await?;
    Ok(row.try_get::<Option<i64>, _>("wiki_last_ingest_at")?)
}

pub async fn set_wiki_last_ingest_at(pool: &SqlitePool, ts: i64) -> Result<()> {
    sqlx::query("UPDATE settings SET wiki_last_ingest_at = ? WHERE id = 1")
        .bind(ts)
        .execute(pool)
        .await?;
    Ok(())
}

pub struct SchedulerConfigRow {
    pub interval_minutes: i64,
    pub date_range: i64,
    pub max_pages: i64,
}

pub async fn get_scheduler_config(pool: &SqlitePool) -> Result<SchedulerConfigRow> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT scheduler_interval_minutes, scheduler_date_range, scheduler_max_pages FROM settings WHERE id = 1"
    )
    .fetch_one(pool)
    .await?;
    Ok(SchedulerConfigRow {
        interval_minutes: row.try_get::<i64, _>("scheduler_interval_minutes")?,
        date_range: row.try_get::<i64, _>("scheduler_date_range")?,
        max_pages: row.try_get::<i64, _>("scheduler_max_pages")?,
    })
}

pub async fn save_scheduler_config(pool: &SqlitePool, config: &SchedulerConfigRow) -> Result<()> {
    sqlx::query(
        "UPDATE settings SET scheduler_interval_minutes = ?, scheduler_date_range = ?, scheduler_max_pages = ? WHERE id = 1"
    )
    .bind(config.interval_minutes)
    .bind(config.date_range)
    .bind(config.max_pages)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Profile lock — unlocked_files list
// ---------------------------------------------------------------------------

pub async fn get_unlocked_files(pool: &SqlitePool) -> Result<Vec<String>> {
    use sqlx::Row;
    let row = sqlx::query("SELECT profile_unlocked_files FROM settings WHERE id = 1")
        .fetch_one(pool)
        .await?;
    let raw: String = row.try_get::<String, _>("profile_unlocked_files").unwrap_or_default();
    Ok(raw.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
}

pub async fn save_unlocked_files(pool: &SqlitePool, files: &[String]) -> Result<()> {
    let joined = files.join(",");
    sqlx::query("UPDATE settings SET profile_unlocked_files = ? WHERE id = 1")
        .bind(joined)
        .execute(pool)
        .await?;
    Ok(())
}
