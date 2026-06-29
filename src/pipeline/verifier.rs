use anyhow::Result;
use serde_json::{json, Value};
use crate::AppState;
use crate::llm::call_llm_tool;
use super::context::build_prompt;

pub async fn verify_call(app: &AppState, master_cv: &str, cv: &Value, max_tokens: u32, reasoning_effort: Option<&str>) -> Result<Value> {
    let context = json!({
        "master_cv": master_cv,
        "draft_cv": cv,
    });
    let task = "You are a rigorous fact-checker. Compare every claim in the CV against \
        the master CV knowledge base. The master CV is the SINGLE SOURCE OF TRUTH.\n\
        \n\
        STEP 1 (do this FIRST): Count every verifiable claim in the CV:\n\
        - N skills (each is one claim)\n\
        - M experiences, each with: company name + role title + K bullet points\n\
        - Summary sentence count\n\
        Total claims = N + M*2 + sum(K) + summary_sentence_count\n\
        STEP 2: Your `items` array MUST have EXACTLY that many entries. No fewer.\n\
        STEP 3: Fill in each entry.\n\
        \n\
        For each claim, determine:\n\
        - truth: directly supported by or reasonably inferred from the master CV\n\
        - lie: cannot be found in the master CV, contradicts it, or exaggerates significantly\n\
        \n\
        IMPORTANT:\n\
        - List EVERY claim. Do not skip any. An incomplete items array is wrong.\n\
        - If a claim is partially true but exaggerated, classify it as 'lie'.\n\
        - Count summary sentences individually. Count each skill as one item. \
          Count each bullet point as one item.\n\
        - CRITICAL: keep `evidence` to ≤12 words. Cite the master CV section \
          briefly (e.g. \"Observability section\") or write \"not mentioned\". \
          NEVER quote long passages verbatim — it bloats the output past the \
          token cap and truncates the JSON.";

    call_llm_tool(app, &build_prompt(task, &context), max_tokens,
        "submit_verification", "Submit your fact-check results", &VERIFY_SCHEMA,
        json!({
            "truth_pct": 67,
            "items": [
                {"category": "skill", "field": "Rust", "claim": "Rust", "verdict": "truth", "evidence": "Rust (primary, 3yr)"},
                {"category": "skill", "field": "Kubernetes", "claim": "Kubernetes", "verdict": "lie", "evidence": "not mentioned"}
            ]
        }),
        reasoning_effort
    ).await
}

static VERIFY_SCHEMA: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| json!({
    "type": "object",
    "properties": {
        "truth_pct": {"type": "integer", "description": "0-100, round(truth_count / total * 100)"},
        "items": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "category": {"type": "string", "enum": ["summary", "skill", "experience"]},
                    "field": {"type": "string", "description": "Company/role or skill name"},
                    "claim": {"type": "string"},
                    "verdict": {"type": "string", "enum": ["truth", "lie"]},
                    "evidence": {"type": "string", "description": "Master CV excerpt or 'not mentioned'"}
                },
                "required": ["category", "field", "claim", "verdict", "evidence"]
            },
            "minItems": 1
        }
    },
    "required": ["truth_pct", "items"]
}));

// ---------------------------------------------------------------------------
// LLM role: Editor
