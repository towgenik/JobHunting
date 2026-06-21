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
    let task = "Analyze master_cv against job_description. \
                Extract and rephrase experiences to match the job requirements.";
    let schema = json!({
        "summary":     "String: 2-3 sentences tailored to the job.",
        "skills":      ["Array of strings: technical skills matching JD"],
        "experiences": [{"company": "String", "role": "String",
                         "bullet_points": ["achievement-focused, quantified"]}]
    });

    let cv = call_llm(app, &build_prompt(task, context, schema)).await?;
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

fn build_prompt(task: &str, context: Value, output_schema: Value) -> String {
    let ctx    = serde_json::to_string_pretty(&context).unwrap_or_default();
    let schema = serde_json::to_string_pretty(&output_schema).unwrap_or_default();
    format!(
        r###"
### CONTEXT
{ctx}
### TASK
{task}
### OUTPUT FORMAT
Return ONLY valid JSON matching this exact structure. No markdown, no explanation.
{schema}
"###
    )
}

async fn call_llm(app: &AppState, _prompt: &str) -> Result<Value> {
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
    // Real LLM call via LLM_ENDPOINT / LLM_API_KEY / LLM_MODEL (M4)
    let resp = app
        .http
        .post(&app.llm_endpoint)
        .bearer_auth(&app.llm_api_key)
        .json(&json!({ "model": app.llm_model, "prompt": _prompt }))
        .send()
        .await?
        .json::<Value>()
        .await?;
    Ok(resp)
}
