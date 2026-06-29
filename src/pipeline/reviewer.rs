use anyhow::Result;
use serde_json::{json, Value};
use crate::AppState;
use crate::llm::call_llm_tool;
use super::context::build_prompt;

pub async fn review_call(app: &AppState, jd: &str, cv: &Value, writer_constraints: Option<&str>, max_tokens: u32, reasoning_effort: Option<&str>) -> Result<Value> {
    let mut context = json!({
        "job_description": jd,
        "draft_cv": cv,
    });
    if let Some(c) = writer_constraints {
        context["writer_constraints"] = json!(c);
    }
    let task = "You are a critical QA reviewer at a top-tier tech company. Review this CV \
        against the job description. Find every weakness, but also know when to ship.\n\
        \n\
        SCORING GUIDE:\n\
        - 90-100: Exceptional CV. Strong match to JD, quantified achievements, \
          no missing critical skills. Ready to submit.\n\
        - 70-89: Good CV with minor issues. Specific improvements possible but not blocking.\n\
        - 50-69: Adequate CV. Significant gaps in JD alignment or weak bullets.\n\
        - Below 50: Poor CV. Major rewriting needed.\n\
        \n\
        SATISFIED SIGNAL — set `satisfied: true` ONLY when ALL are true:\n\
        1. Score is 85+\n\
        2. No critical JD requirements missing from CV (unless the writer reports a \
           master CV limitation in `writer_constraints` — in that case, the gap is \
           acceptable and does not block satisfaction)\n\
        3. Every bullet point is quantified\n\
        4. Summary leads with the most relevant strength for THIS role\n\
        If ANY fail, set `satisfied: false` with specific feedback.\n\
        \n\
        If `writer_constraints` is present in the context, the writer has explained \
        why certain reviewer feedback cannot be addressed. Factor this into your \
        satisfied decision — do not keep requesting what the master CV cannot provide.";

    call_llm_tool(app, &build_prompt(task, &context), max_tokens,
        "submit_review", "Submit your CV review", &REVIEW_SCHEMA,
        json!({"score": 92, "feedback": "Mock: strong draft — passes.", "strengths": "All.", "satisfied": true}),
        reasoning_effort
    ).await
}

static REVIEW_SCHEMA: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| json!({
    "type": "object",
    "properties": {
        "score": {"type": "integer", "description": "0-100"},
        "feedback": {"type": "string", "description": "Specific, actionable critique"},
        "strengths": {"type": "string", "description": "What to keep across revision"},
        "satisfied": {"type": "boolean", "description": "True if no actionable critique remains"}
    },
    "required": ["score", "feedback", "strengths", "satisfied"]
}));

// ---------------------------------------------------------------------------
// LLM role: Verifier
