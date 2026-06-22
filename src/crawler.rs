use anyhow::Result;
use uuid::Uuid;
use crate::{db, generate, AppState};

/// Background task: crawl a listing URL, create job stubs, process each sequentially.
/// Sequential processing avoids overwhelming the local LLM (single-instance llama.cpp).
pub async fn run_search(app: AppState, search_id: Uuid, listing_url: String) {
    // 1. Crawl listing page → JSON array of detail URLs
    let urls: Vec<String> = match crawl_listing(&listing_url).await {
        Ok(urls) => urls,
        Err(e) => {
            eprintln!("search {search_id}: crawl failed: {e}");
            return;
        }
    };

    if urls.is_empty() {
        eprintln!("search {search_id}: no URLs found");
        return;
    }

    let found_count = urls.len() as i64;
    if let Err(e) = db::create_search(&app.db, search_id, &listing_url, found_count).await {
        eprintln!("search {search_id}: create_search failed: {e}");
        return;
    }

    // 2. Process each discovered URL sequentially.
    //    Sequential is intentional — the local LLM (llama.cpp) processes one
    //    request at a time; parallel spawns would queue up and time out.
    for url in &urls {
        // Duplicate check: if this URL is already in the DB, skip creating a new job.
        match db::get_job_id_by_url(&app.db, url).await {
            Ok(Some(existing_id)) => {
                // Existing job — link it to this search if not already linked.
                // We don't re-process it; the user already has the CV (or it's still
                // running). Just ensure it's associated with this search.
                let _ = db::link_job_to_search(&app.db, existing_id, search_id).await;
                eprintln!("search {search_id}: skipping duplicate {url} (job {existing_id})");
                continue;
            }
            Ok(None) => { /* new URL, proceed */ }
            Err(e) => {
                eprintln!("search {search_id}: dup check failed for {url}: {e}");
                continue;
            }
        }

        let job_id = match db::create_job_stub_for_search(&app.db, url, search_id).await {
            Ok(id) => id,
            Err(e) => {
                eprintln!("search {search_id}: create_job_stub failed for {url}: {e}");
                continue;
            }
        };

        // Process this job to completion before moving to the next.
        // This limits concurrency to 1 LLM call at a time.
        if let Err(e) = generate::process_job(&app, job_id).await {
            eprintln!("search {search_id}: process_job {job_id} failed: {e}");
            let _ = db::set_status(&app.db, job_id, "failed").await;
        }
    }

    eprintln!("search {search_id}: done — {found_count} URLs processed");
}

/// Spawn crawl_listing.py and parse its JSON-array output.
async fn crawl_listing(listing_url: &str) -> Result<Vec<String>> {
    let out = tokio::process::Command::new("python3")
        .arg("crawl_listing.py")
        .arg(listing_url)
        .output()
        .await?;
    anyhow::ensure!(
        out.status.success(),
        "crawl_listing.py failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let urls: Vec<String> = serde_json::from_slice(&out.stdout)?;
    Ok(urls)
}
