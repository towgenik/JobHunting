use anyhow::Result;
use serde_json::Value;
use uuid::Uuid;
use crate::{db, events, finish_crawl_activity, generate, set_crawl_activity, AppState};

/// Shared per-URL processing loop. Dedup check → create stub → process job.
/// Jobs are spawned concurrently — the LLM semaphore limits actual parallelism.
/// Cancel checks happen between spawns (the in-flight LLM calls run to completion).
async fn process_discovered_urls(app: &AppState, search_id: Uuid, urls: Vec<String>) {
    let total = urls.len();
    let mut handles = Vec::with_capacity(urls.len());
    let mut spawned = 0usize;

    for url in urls.iter() {
        if app.crawl_cancel.load(std::sync::atomic::Ordering::Relaxed) {
            let msg = format!("Stopped after {spawned}/{total} jobs");
            set_crawl_activity(app, Some(search_id), &msg);
            eprintln!("search {search_id}: cancelled at {spawned}/{total}");
            break;
        }

        let normalized_url = normalize_url(url);

        match db::get_job_id_by_url(&app.db, &normalized_url).await {
            Ok(Some(existing_id)) => {
                let _ = db::link_job_to_search(&app.db, existing_id, search_id).await;
                eprintln!("search {search_id}: skipping duplicate {url} (job {existing_id})");
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("search {search_id}: dup check failed for {url}: {e}");
                continue;
            }
        }

        let job_id = match db::create_job_stub_for_search(&app.db, &normalized_url, search_id).await {
            Ok(id) => {
                events::publish_job_update(app, id, "new", "Queued for screening");
                id
            }
            Err(e) => {
                eprintln!("search {search_id}: create_job_stub failed for {url}: {e}");
                continue;
            }
        };

        let app_clone = app.clone();
        let hint = url_hint(url);
        spawned += 1;
        set_crawl_activity(&app_clone, Some(search_id), &format!("Spawned {spawned}/{total}: {hint}"));
        events::publish_crawl_progress(app, &format!("Spawned {spawned}/{total}: {hint}"));

        handles.push(tokio::spawn(async move {
            if let Err(e) = generate::process_job(&app_clone, job_id).await {
                eprintln!("search {search_id}: process_job {job_id} failed: {e}");
                let _ = db::delete_job(&app_clone.db, job_id).await;
            }
        }));
    }

    // Wait for all spawned jobs to complete.
    let remaining = handles.len();
    for (i, h) in handles.into_iter().enumerate() {
        let _ = h.await;
        set_crawl_activity(
            app,
            Some(search_id),
            &format!("Completed {}/{total} jobs", spawned - remaining + i + 1),
        );
    }

    set_crawl_activity(app, Some(search_id), &format!("Completed {spawned}/{total} jobs"));
    events::publish_crawl_finished(app);
    finish_crawl_activity(app);
}

/// Short human label for a job URL: last path segment, URL-decoded.
/// JobStreet URLs look like `…/jobs/50123456/senior-backend-engineer-at-acme`.
fn url_hint(url: &str) -> String {
    let last = url.rsplit('/').next().unwrap_or(url);
    let decoded = percent_decode(last);
    if decoded.is_empty() { url.chars().take(40).collect() } else { decoded }
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                out.push(b as char);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' { out.push(' '); } else { out.push(bytes[i] as char); }
        i += 1;
    }
    out.replace('-', " ")
}

/// Firehose browse: empty keywords, date range, sort by most recent.
/// Called by the scheduler. Pre-filters by title blacklist, dedup checks, then
/// processes each surviving job sequentially through the LLM pipeline.
pub async fn scheduler_browse(app: AppState, date_range: u32, max_pages: u32) -> Result<()> {
    app.crawl_cancel
        .store(false, std::sync::atomic::Ordering::Relaxed);
    let sid = Uuid::new_v4();
    if let Ok(mut a) = app.crawl_activity.write() {
        a.active = true;
        a.stopping = false;
    }
    set_crawl_activity(&app, Some(sid), &format!("Scheduler: browsing last {date_range} days"));

    // ponytail: 300s subprocess timeout. index_api.py has 30s per-request timeout,
    // but with 5 pages × 30s + delays the total can reach ~155s. 300s gives headroom
    // for DNS stalls or interpreter hangs without blocking the crawl forever.
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        tokio::process::Command::new("python3")
            .arg("index_api.py")
            .arg("")
            .arg("--date-range").arg(date_range.to_string())
            .arg("--sort").arg("ListedDate")
            .arg("--pages").arg(max_pages.to_string())
            .arg("--page-size").arg("100")
            .arg("--site-key").arg("ID")
            .arg("--locale").arg("id-ID")
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("index_api.py timed out after 300s"))??;
    anyhow::ensure!(
        out.status.success(),
        "index_api.py failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let jobs: Vec<Value> = serde_json::from_slice(&out.stdout)?;
    if jobs.is_empty() {
        eprintln!("scheduler_browse: no jobs found for date_range={date_range}");
        set_crawl_activity(&app, Some(sid), "Scheduler: no jobs found");
        events::publish_crawl_finished(&app);
        finish_crawl_activity(&app);
        return Ok(());
    }

    let found_count = jobs.len() as i64;
    let blacklist = &app.profile_title_blacklist;
    let mut filtered_count = 0i64;
    let mut urls_to_process = Vec::new();

    for job in &jobs {
        let title = job["title"].as_str().unwrap_or("");
        let url = job["url"].as_str().unwrap_or("");
        if title_blacklist_match(title, blacklist) {
            eprintln!("scheduler_browse: filtered (title blacklist): {title}");
            filtered_count += 1;
            continue;
        }
        urls_to_process.push(url.to_string());
    }

    eprintln!("scheduler_browse: {found_count} found, {filtered_count} filtered, {} proceeding",
              urls_to_process.len());

    if urls_to_process.is_empty() {
        events::publish_crawl_finished(&app);
        finish_crawl_activity(&app);
        return Ok(());
    }

    // Cap per-crawl job count from pipeline settings
    let pipeline = db::get_pipeline_config(&app.db).await.unwrap_or_default();
    let cap = pipeline.max_jobs_per_crawl.clamp(5, 500) as usize;
    if urls_to_process.len() > cap {
        eprintln!("scheduler_browse: capping from {} to {cap} (max_jobs_per_crawl)", urls_to_process.len());
        urls_to_process.truncate(cap);
    }

    let search_id = sid;
    let listing_url = format!("dateRange={date_range}&sortMode=ListedDate");
    db::create_search(&app.db, search_id, &listing_url, found_count).await?;

    process_discovered_urls(&app, search_id, urls_to_process).await;

    eprintln!("scheduler_browse: done");
    events::publish_crawl_finished(&app);
    finish_crawl_activity(&app);
    Ok(())
}

/// Check if a job title matches any blacklisted keyword (case-insensitive).
fn title_blacklist_match(title: &str, blacklist: &[String]) -> bool {
    let title_lower = title.to_lowercase();
    for keyword in blacklist {
        if title_lower.contains(&keyword.to_lowercase()) {
            return true;
        }
    }
    false
}

/// Normalize URL by stripping query parameters for dedup.
/// SEEK recycles URLs with ?tracking=... parameters.
fn normalize_url(url: &str) -> String {
    match url.find('?') {
        Some(pos) => url[..pos].to_string(),
        None => url.to_string(),
    }
}
