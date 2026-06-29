use anyhow::Result;
use serde_json::{json, Value};
use crate::AppState;
use crate::llm::call_llm_tool;
use super::context::build_prompt;

pub async fn editor_call(app: &AppState, master_cv: &str, cv: &Value, max_tokens: u32, reasoning_effort: Option<&str>) -> Result<Value> {
    let context = json!({
        "master_cv": master_cv,
        "draft_cv": cv,
    });
    let task = "You are a FACT-CHECKER, not a CV writer. You are a DIFFERENT person than \
        the writer who created this draft. The writer was incentivized to embellish; you are \
        incentivized to be accurate.\n\
        \n\
        RULES:\n\
        - If a bullet point makes a claim not EXPLICITLY in the master CV, DELETE it.\n\
          Do not soften, hedge, or rewrite. DELETE.\n\
        - If an experience has no supporting facts, remove it entirely.\n\
        - bullet_points may be empty [] — that is acceptable and preferred over fabrication.\n\
        - DO NOT apply the 'experiences MUST contain ≥1' rule — that is the writer's \
          constraint, not yours.\n\
        - Re-output the FULL CV with these corrections applied.";

    call_llm_tool(app, &build_prompt(task, &context), max_tokens,
        "submit_edited_cv", "Submit the corrected CV", &EDITOR_SCHEMA,
        json!({
            "summary": "Mock edited summary.",
            "skills": ["Rust", "Python"],
            "experiences": [{"company": "Mock Corp", "role": "Mock Engineer", "bullet_points": ["Edited mock bullet."]}]
        }),
        reasoning_effort
    ).await
}

static EDITOR_SCHEMA: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| json!({
    "type": "object",
    "properties": {
        "summary": {"type": "string"},
        "skills": {"type": "array", "items": {"type": "string"}},
        "experiences": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "company": {"type": "string"},
                    "role": {"type": "string"},
                    "bullet_points": {"type": "array", "items": {"type": "string"}}
                },
                "required": ["company", "role", "bullet_points"]
            }
        }
    },
    "required": ["summary", "skills", "experiences"]
}));

// ---------------------------------------------------------------------------
// LLM role: Ranker
