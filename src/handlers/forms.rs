use serde::Deserialize;

#[derive(Deserialize)]
pub struct JobForm {
    pub url: String,
}

#[derive(Deserialize)]
pub struct RegenerateForm {
    pub review_notes: Option<String>,
}

#[derive(Deserialize)]
pub struct LlmSettingsForm {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub openai_compat: Option<String>,
    pub mock_llm: Option<String>,
}

#[derive(Deserialize)]
pub struct SchedulerSettingsForm {
    pub interval_minutes: i64,
    pub date_range: i64,
    pub max_pages: i64,
}

#[derive(Deserialize)]
pub struct AgentSettingsForm {
    pub ctx_window:          i64,
    pub max_output:          i64,
    pub thinking_effort:     String,
    pub wiki_query_max_hops: i64,
    pub wiki_auto_ingest:    Option<String>,
}

#[derive(Deserialize)]
pub struct ProfileForm {
    pub content: String,
    pub file: String,
}

// ponytail: DeleteBatchForm removed — serde_urlencoded can't parse repeated
