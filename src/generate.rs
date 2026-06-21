use anyhow::Result;
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};
use uuid::Uuid;
use crate::{AppState, db};

pub async fn process_job(app: &AppState, job_id: Uuid) -> Result<()> {
    db::set_status(&app.db, job_id, "scraping").await?;

    let url      = db::get_job_url(&app.db, job_id).await?;
    let job_data = fetch_job(&url).await?;
    db::update_job_data(&app.db, job_id, &job_data).await?;

    db::set_status(&app.db, job_id, "generating").await?;

    let master_cv = db::get_master_cv(&app.db).await?;
    let context   = json!({
        "job_description": job_data["description"],
        "master_cv":       master_cv,
    });
    // ponytail: DeepSeek (OpenRouter) silently returned experiences:[] on the loose
    // wording — fixed M9 by (1) an explicit MUST in TASK, (2) field-by-field schema
    // with minItems language, (3) a one-shot example. See Architecture §5.5.
    let task = "Analyze master_cv against job_description and produce a tailored CV draft.\n\
                \n\
                HARD REQUIREMENTS:\n\
                1. `summary` MUST be 2-3 sentences tailored to the job.\n\
                2. `skills` MUST list 5-15 technical skills drawn from the JD.\n\
                3. `experiences` MUST contain AT LEAST ONE (≥1) entry — an empty array is a FAILURE.\n\
                   Build each entry from the master_cv's work history, rephrased and re-ordered to\n\
                   match the JD. If the master_cv has no explicit work history, synthesize entries\n\
                   from any project, education, or role-like content it contains — never return [].\n\
                   Each entry needs `company`, `role`, and 2-5 `bullet_points` that are quantified\n\
                   and achievement-focused, echoing the JD's required responsibilities.";
    let schema = json!({
        "summary":     "String (required): 2-3 sentences tailored to the job.",
        "skills":      "Array of strings (required, 5-15 items): technical skills matching the JD.",
        "experiences": "Array of objects (REQUIRED, MIN LENGTH 1 — never []). \
                        Each object has exactly these fields:\n\
                        - \"company\":       String (the employer name)\n\
                        - \"role\":          String (the job title)\n\
                        - \"bullet_points\": Array of strings (2-5 items, quantified achievements\n\
                                            that echo the JD's responsibilities)"
    });
    let example = json!({
        "summary": "Senior backend engineer with 6+ years building high-throughput Rust services.",
        "skills": ["Rust", "Tokio", "PostgreSQL", "AWS", "gRPC"],
        "experiences": [{
            "company": "Acme Corp",
            "role":    "Senior Software Engineer",
            "bullet_points": [
                "Cut p99 latency 40% by rewriting the billing pipeline in Rust/Tokio.",
                "Owned the migration from Node.js to Rust across 8 services."
            ]
        }]
    });

    let cv = call_llm(app, &build_prompt(task, context, schema, example)).await?;
    db::save_cv_draft(&app.db, job_id, cv).await?;
    db::set_status(&app.db, job_id, "pending_approval").await?;
    Ok(())
}

// ponytail: subprocess, 3s courtesy delay + one retry; add backoff if bot-detection bites
async fn fetch_job(url: &str) -> Result<Value> {
    sleep(Duration::from_secs(3)).await;
    if let Ok(v) = scrape_once(url).await {
        return Ok(v);
    }
    sleep(Duration::from_secs(10)).await;
    scrape_once(url).await
}

async fn scrape_once(url: &str) -> Result<Value> {
    let out = tokio::process::Command::new("python")
        .arg("scrape.py")
        .arg(url)
        .output()
        .await?;
    anyhow::ensure!(
        out.status.success(),
        "scrape.py failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(serde_json::from_slice(&out.stdout)?)
}

fn build_prompt(task: &str, context: Value, output_schema: Value, example: Value) -> String {
    let ctx     = serde_json::to_string_pretty(&context).unwrap_or_default();
    let schema  = serde_json::to_string_pretty(&output_schema).unwrap_or_default();
    let example = serde_json::to_string_pretty(&example).unwrap_or_default();
    format!(
        r###"
### CONTEXT
{ctx}
### TASK
{task}
### OUTPUT FORMAT
Return ONLY valid JSON matching this exact structure. No markdown, no explanation, no prose before or after.
{schema}

### EXAMPLE OUTPUT (shape reference — replace content with JD-derived material)
{example}

Remember: `experiences` MUST contain at least one entry. Returning `[]` for experiences is a failure.
"###
    )
}

async fn call_llm(app: &AppState, prompt: &str) -> Result<Value> {
    if app.mock_llm {
        // ponytail: mock returns valid structure so full UI flow works offline
        return Ok(json!({
            "summary": "Mock summary for development.",
            "skills": ["Rust", "Python", "PostgreSQL"],
            "experiences": [{
                "company": "Mock Corp",
                "role": "Mock Engineer",
                "bullet_points": ["Achieved mock results", "Delivered mock features"]
            }]
        }));
    }

    // Real LLM call — supports Anthropic and OpenAI-compatible APIs.
    // Auto-detected from LLM_ENDPOINT; override with LLM_PROVIDER=anthropic|openai.
    // ponytail: both use the same request body shape; only auth header + response path differ.
    let body = json!({
        "model": app.llm_model,
        // ponytail: was 2048 — the M9 prompt (explicit fields + example + ≥1 experience
        // requirement) makes DeepSeek emit longer, more thorough CVs that overran 2048
        // and arrived truncated mid-JSON ("EOF while parsing"). 4096 leaves headroom for
        // 3 experiences × 5 bullets. OpenAI newer models use max_completion_tokens; this
        // key works on DeepSeek/OpenRouter today.
        "max_tokens": 4096,
        "messages": [{"role": "user", "content": prompt}]
    });

    let req = app.http.post(&app.llm_endpoint).json(&body);
    let req = if app.openai_compat {
        req.bearer_auth(&app.llm_api_key)
    } else {
        // Anthropic: x-api-key header + required anthropic-version header
        req.header("x-api-key", &app.llm_api_key)
           .header("anthropic-version", "2023-06-01")
    };

    let api_resp = req.send().await?;
    let status = api_resp.status();
    let resp: Value = api_resp.json().await?;

    if !status.is_success() {
        anyhow::bail!(
            "LLM API error {}: {}",
            status,
            resp.get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
        );
    }

    // Extract the response text — path differs by provider.
    let text = if app.openai_compat {
        resp.pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("LLM response missing choices[0].message.content: {resp}"))?
    } else {
        resp.pointer("/content/0/text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("LLM response missing content[0].text: {resp}"))?
    };

    let cv: Value = serde_json::from_str(text)
        .map_err(|e| anyhow::anyhow!("LLM returned non-JSON text: {e}\nraw: {text}"))?;

    Ok(cv)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Self-check: `build_prompt` output contains all required sections and the
    /// M9 anti-regression guard (the "experiences MUST contain at least one"
    /// line). If the format changes the LLM stops receiving the context it needs.
    #[test]
    fn build_prompt_contains_required_sections() {
        let context = json!({"job_description": "Engineer role", "master_cv": "CV text"});
        let schema  = json!({"summary": "string", "skills": [], "experiences": []});
        let example = json!({"experiences": [{"company": "X", "role": "Y", "bullet_points": ["z"]}]});
        let prompt  = build_prompt("Do the thing", context, schema, example);

        assert!(prompt.contains("### CONTEXT"),     "missing CONTEXT section");
        assert!(prompt.contains("### TASK"),        "missing TASK section");
        assert!(prompt.contains("OUTPUT FORMAT"),   "missing OUTPUT FORMAT section");
        assert!(prompt.contains("EXAMPLE OUTPUT"),  "missing EXAMPLE section");
        assert!(prompt.contains("Do the thing"),    "task text not in prompt");
        assert!(prompt.contains("job_description"), "context not serialized into prompt");
        // M9 guard: the experiences-non-empty reminder must survive in the prompt.
        assert!(prompt.contains("MUST contain at least one entry"),
                "missing experiences non-empty reminder — M9 regression");
    }

    /// Self-check: mock LLM output has the three required top-level keys.
    /// The `process_job` pipeline will panic at template-render time if any of
    /// these are missing, so catch it here instead of at runtime.
    #[tokio::test]
    async fn mock_llm_returns_required_keys() {
        // Build a minimal AppState with mock_llm = true; no real DB or HTTP needed.
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory db");
        let app = crate::AppState {
            db:            pool,
            http:          reqwest::Client::new(),
            llm_endpoint:  String::new(),
            llm_api_key:   String::new(),
            llm_model:     String::new(),
            mock_llm:      true,
            openai_compat: false,
        };
        let result = call_llm(&app, "ignored prompt").await.expect("mock must not fail");
        assert!(result["summary"].is_string(),   "mock missing 'summary'");
        assert!(result["skills"].is_array(),     "mock missing 'skills'");
        assert!(result["experiences"].is_array(),"mock missing 'experiences'");
    }
}
