use anyhow::Result;
use serde_json::{json, Value};
use crate::AppState;

pub async fn fetch_job(app: &AppState, url: &str) -> Result<Value> {
    if app.llm_config.read().unwrap_or_else(|e| e.into_inner()).mock_llm {
        return Ok(json!({
            "title": "Mock Backend Engineer",
            "description": "We are looking for a backend engineer with experience in Rust, Python, and Docker. Requirements: 3+ years of experience, strong knowledge of APIs, experience with PostgreSQL.",
            "company": "Mock Corp"
        }));
    }
    scrape_once(url).await
}

async fn scrape_once(url: &str) -> Result<Value> {
    // ponytail: 60s subprocess timeout. scrape_api.py has its own 15s HTTP
    // timeout, but if the Python interpreter itself hangs (DNS, import, OOM),
    // .output().await waits forever without this.
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        tokio::process::Command::new("python3")
            .arg("scrape_api.py")
            .arg(url)
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("scrape_api.py timed out after 60s"))??;
    anyhow::ensure!(
        out.status.success(),
        "scrape_api.py failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(serde_json::from_slice(&out.stdout)?)
}
