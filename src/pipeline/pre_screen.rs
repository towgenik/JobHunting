use anyhow::Result;
use serde_json::{json, Value};
use crate::AppState;
use super::context::build_prompt;

// ---------------------------------------------------------------------------
// Pipeline: scrape → pre-screen → write → review loop → verify → editor → rank → save
// ---------------------------------------------------------------------------

static PRE_SCREEN_SCHEMA: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| json!({
    "type": "object",
    "properties": {
        "score": {"type": "integer", "description": "0-100 fit score"},
        "category": {"type": "string", "enum": ["good_match", "possible", "wrong_role", "wrong_industry", "wrong_level", "missing_skills"]}
    },
    "required": ["score", "category"]
}));

/// Quick pre-screening call — 1 LLM request, cheap.
pub async fn pre_screen(
    app: &AppState,
    master_cv: &str,
    title: &str,
    description: &str,
    max_tokens: u32,
    reasoning_effort: Option<&str>,
    job_id: Option<uuid::Uuid>,
) -> Result<(i64, String)> {
    let context = json!({
        "master_cv": master_cv,
        "job_title": title,
        "job_description": description.chars().take(1500).collect::<String>(),
    });
    let task = "Score 0-100 how well this candidate fits this role:\n\
        - 70-100: good_match — right skills, role type, experience.\n\
        - 40-69: possible — some overlap, worth exploring.\n\
        - 0-39: one of wrong_role, wrong_industry, wrong_level, missing_skills.\n\
        \n\
        Return ONLY {score, category}. No descriptions, no explanation. One line.";

    let result = crate::llm::call_llm_tool_with_progress(app, &build_prompt(task, &context), max_tokens,
        "submit_review", "Submit your pre-screen verdict", &PRE_SCREEN_SCHEMA,
        json!({"score": 50, "category": "possible"}),
        reasoning_effort,
        job_id,
        Some("Pre-screening: checking fit…"),
    ).await?;

    let score = result["score"].as_i64().unwrap_or(50).clamp(0, 100);
    let category = result["category"].as_str().unwrap_or("missing_skills").to_string();
    Ok((score, category))
}
