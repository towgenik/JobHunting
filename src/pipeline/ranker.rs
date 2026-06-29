use anyhow::Result;
use serde_json::{json, Value};
use crate::AppState;
use crate::llm::call_llm_tool;
use super::context::build_prompt;

pub async fn ranker_call(app: &AppState, jd: &str, cv: &Value, max_tokens: u32, reasoning_effort: Option<&str>) -> Result<Value> {
    let context = json!({
        "job_description": jd,
        "cv": cv,
    });
    let task = "You are predicting whether a real HR manager would shortlist this CV \
        for this specific job. This is NOT a quality score — it's a prediction of \
        real-world hiring outcome.\n\
        \n\
        A high-quality CV (90+) for a senior role when the candidate has 2 years of \
        experience might have approval_probability of 25.\n\
        A mediocre CV (70) for an entry-level role in a desperate market might have \
        approval_probability of 80.\n\
        \n\
        Calibration:\n\
        - 80-100: Strong match. HR would likely shortlist.\n\
        - 50-79: Possible. Depends on competition.\n\
        - 20-49: Unlikely. Significant gaps.\n\
        - 0-19: Very unlikely. Wrong level or implausible claims.";

    call_llm_tool(app, &build_prompt(task, &context), max_tokens,
        "submit_ranking", "Submit your HR approval prediction", &RANK_SCHEMA,
        json!({
            "approval_probability": 72,
            "good": ["Strong Rust experience", "Quantified achievements"],
            "bad": ["Missing Kubernetes"],
            "improvements": ["Add Kubernetes if you have it"]
        }),
        reasoning_effort
    ).await
}

static RANK_SCHEMA: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| json!({
    "type": "object",
    "properties": {
        "approval_probability": {"type": "integer", "description": "0-100"},
        "good": {"type": "array", "items": {"type": "string"}, "description": "What the CV does well for this role"},
        "bad": {"type": "array", "items": {"type": "string"}, "description": "What's missing or weak"},
        "improvements": {"type": "array", "items": {"type": "string"}, "description": "Actionable improvements"}
    },
    "required": ["approval_probability", "good", "bad", "improvements"]
}));

// ---------------------------------------------------------------------------
// Scraper
