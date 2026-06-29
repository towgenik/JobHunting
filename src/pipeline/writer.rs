use anyhow::Result;
use serde_json::{json, Value};
use crate::AppState;
use crate::llm::call_llm_tool;
use super::context::build_prompt;

pub async fn writer_call(
    app: &AppState,
    master_cv: &str,
    jd: &str,
    prior_review: Option<&Value>,
    max_tokens: u32,
    reasoning_effort: Option<&str>,
) -> Result<Value> {
    let context = if let Some(hr) = prior_review {
        json!({
            "master_cv": master_cv,
            "job_description": jd,
            "prior_draft_feedback": {
                "score":    hr["score"],
                "feedback": hr["feedback"],
                "strengths": hr["strengths"],
            },
        })
    } else {
        json!({ "master_cv": master_cv, "job_description": jd })
    };

    let task = if prior_review.is_some() {
        format!(
            "REVISE the prior draft to fix every point in the recruiter feedback below.\n\
             Do NOT discard strengths; only address weaknesses.\n\n{}",
            WRITER_TASK
        )
    } else {
        WRITER_TASK.to_string()
    };

    call_llm_tool(app, &build_prompt(&task, &context), max_tokens,
        "submit_cv", "Submit the tailored CV", &WRITER_SCHEMA,
        json!({
            "summary": "Mock summary for development.",
            "skills": ["Rust", "Python", "PostgreSQL"],
            "experiences": [{
                "company": "Mock Corp",
                "role": "Mock Engineer",
                "bullet_points": ["Achieved mock results", "Delivered mock features"]
            }]
        }),
        reasoning_effort
    ).await
}

const WRITER_TASK: &str = "You are a CV writer. Given a job description and a master CV (knowledge base),\n\
    produce a tailored CV that makes the candidate look like the obvious hire.\n\
    \n\
    RULES:\n\
    1. Cherry-pick — only include experience, skills, and projects from the master CV\n\
       that are RELEVANT to this specific job. Skip unrelated content entirely.\n\
    2. Match the JD's language — use the same terms the job uses.\n\
    3. Quantify every bullet point — use real numbers from the master CV if available.\n\
    4. Reorder experience — most relevant role first, regardless of chronology.\n\
    5. The output must be concise enough to fit one A4 page.\n\
    \n\
    HARD REQUIREMENTS:\n\
    - `summary`: 2-3 sentences. Lead with the candidate's most relevant strength for THIS role.\n\
    - `skills`: 5-15 technical skills from the JD that the candidate actually has.\n\
    - `experiences`: ≥1 entry (empty array = FAILURE). Each: `company`, `role`,\n\
      `bullet_points` (2-5 quantified, achievement-focused bullets).\n\
    - `constraints`: If the reviewer asks for something the master CV cannot provide\n\
      (missing skill, no relevant experience), explain what you cannot fix and why.\n\
      Omit or set to null if no constraints.";

static WRITER_SCHEMA: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| json!({
    "type": "object",
    "properties": {
        "summary": {"type": "string", "description": "2-3 sentences tailored to the job"},
        "skills": {"type": "array", "items": {"type": "string"}, "description": "5-15 JD-matching skills the candidate has"},
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
            },
            "description": "≥1 entry, never empty"
        },
        "constraints": {"type": "string", "description": "What the reviewer asked for that the master CV cannot provide, and why. Omit if none."}
    },
    "required": ["summary", "skills", "experiences"]
}));

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_llm_returns_required_keys() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory db");
        let llm_config = std::sync::Arc::new(std::sync::RwLock::new(crate::LlmConfig {
            endpoint: String::new(),
            api_key: String::new(),
            model: String::new(),
            openai_compat: false,
            mock_llm: true,
        }));
        let app = crate::AppState {
            db:               pool,
            http:             reqwest::Client::new(),
            llm_config,
            llm_semaphore:    std::sync::Arc::new(tokio::sync::Semaphore::new(2)),
            scheduler_running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            last_scheduler_run: std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0)),
            profile_title_blacklist: vec![],
            profile_deal_breaker_keywords: vec![],
            crawl_cancel:     std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            crawl_activity:   std::sync::Arc::new(std::sync::RwLock::new(crate::CrawlActivity::default())),
            event_bus:        tokio::sync::broadcast::channel::<String>(256).0,
            wiki:             std::sync::Arc::new(std::sync::RwLock::new(None)),
        };
        let result = writer_call(&app, "master cv", "job desc", None, 16384, None).await.expect("mock must not fail");
        assert!(result["summary"].is_string(),   "mock missing 'summary'");
        assert!(result["skills"].is_array(),     "mock missing 'skills'");
        assert!(result["experiences"].is_array(),"mock missing 'experiences'");
    }
}

// ---------------------------------------------------------------------------
// LLM role: Reviewer
