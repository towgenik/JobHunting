use anyhow::Result;
use sqlx::SqlitePool;
use uuid::Uuid;
use serde_json::Value;
use crate::templates::CvReadyTemplate;

/// Create a stub job record with status 'new'. Returns the new job ID.
pub async fn create_job_stub(pool: &SqlitePool, url: &str) -> Result<Uuid> {
    let id = Uuid::new_v4();
    let id_str = id.to_string();
    sqlx::query(
        "INSERT INTO jobs (id, url, status, search_id, created_at) VALUES (?, ?, 'new', NULL, datetime('now'))"
    )
    .bind(&id_str)
    .bind(url)
    .execute(pool)
    .await?;
    Ok(id)
}

/// Create a stub job record with a search_id link.
pub async fn create_job_stub_for_search(
    pool: &SqlitePool,
    url: &str,
    search_id: Uuid,
) -> Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO jobs (id, url, status, search_id, created_at) VALUES (?, ?, 'new', ?, datetime('now'))"
    )
    .bind(&id.to_string())
    .bind(url)
    .bind(&search_id.to_string())
    .execute(pool)
    .await?;
    Ok(id)
}

/// Set the status column for a job.
pub async fn set_status(pool: &SqlitePool, job_id: Uuid, status: &str) -> Result<()> {
    sqlx::query("UPDATE jobs SET status = ? WHERE id = ?")
        .bind(status)
        .bind(job_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Write a live progress message for the polling card. Shown during generation.
pub async fn set_progress(pool: &SqlitePool, job_id: Uuid, msg: &str) -> Result<()> {
    sqlx::query("UPDATE jobs SET progress = ? WHERE id = ?")
        .bind(msg)
        .bind(job_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Retrieve the URL for a job.
pub async fn get_job_url(pool: &SqlitePool, job_id: Uuid) -> Result<String> {
    let row = sqlx::query("SELECT url FROM jobs WHERE id = ?")
        .bind(job_id.to_string())
        .fetch_one(pool)
        .await?;
    use sqlx::Row;
    Ok(row.try_get("url")?)
}

/// Update title and description after scraping.
pub async fn update_job_data(pool: &SqlitePool, job_id: Uuid, data: &Value) -> Result<()> {
    let title = data["title"].as_str().unwrap_or("").to_string();
    let description = data["description"].as_str().unwrap_or("").to_string();
    let company = data["company"].as_str().unwrap_or("").to_string();
    sqlx::query("UPDATE jobs SET title = ?, description = ?, company = ? WHERE id = ?")
        .bind(&title)
        .bind(&description)
        .bind(&company)
        .bind(job_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Get the current status string for a job. Returns empty string if not found.
pub async fn get_status(pool: &SqlitePool, job_id: Uuid) -> Result<String> {
    use sqlx::Row;
    let row = sqlx::query("SELECT status FROM jobs WHERE id = ?")
        .bind(job_id.to_string())
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.try_get::<String, _>("status").unwrap_or_default()).unwrap_or_default())
}

/// Get url + progress for the job card. Returns ("", "") if not found.
pub async fn get_job_card_data(pool: &SqlitePool, job_id: Uuid) -> Result<(String, String)> {
    use sqlx::Row;
    let row = sqlx::query("SELECT url, progress FROM jobs WHERE id = ?")
        .bind(job_id.to_string())
        .fetch_optional(pool)
        .await?;
    match row {
        None => Ok((String::new(), String::new())),
        Some(r) => {
            let url = r.try_get::<Option<String>, _>("url")?.unwrap_or_default();
            let progress = r.try_get::<Option<String>, _>("progress")?.unwrap_or_default();
            Ok((url, progress))
        }
    }
}

/// Save the LLM-generated CV JSON to the job record.
pub async fn save_cv_draft(pool: &SqlitePool, job_id: Uuid, cv: Value) -> Result<()> {
    let cv_str = cv.to_string();
    sqlx::query("UPDATE jobs SET cv = ? WHERE id = ?")
        .bind(&cv_str)
        .bind(job_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Get the master CV from settings (always row id=1).
pub async fn get_master_cv(pool: &SqlitePool) -> Result<String> {
    use sqlx::Row;
    let row = sqlx::query("SELECT master_cv FROM settings WHERE id = 1")
        .fetch_one(pool)
        .await?;
    Ok(row.try_get("master_cv")?)
}

/// Build and return the CvReadyTemplate for a job at pending_approval status.
pub async fn render_cv_ready(pool: &SqlitePool, job_id: Uuid) -> CvReadyTemplate {
    use sqlx::Row;
    let row = sqlx::query("SELECT title FROM jobs WHERE id = ?")
        .bind(job_id.to_string())
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    let title = row
        .and_then(|r| r.try_get::<Option<String>, _>("title").ok().flatten())
        .unwrap_or_else(|| "(no title)".to_string());
    CvReadyTemplate { id: job_id, title }
}

/// Upsert the master CV in settings.
#[allow(dead_code)]
pub async fn upsert_master_cv(pool: &SqlitePool, master_cv: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (id, master_cv) VALUES (1, ?)
         ON CONFLICT(id) DO UPDATE SET master_cv = excluded.master_cv"
    )
    .bind(master_cv)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get a full job row for the review page.
pub async fn get_job(pool: &SqlitePool, job_id: Uuid) -> Result<JobRecord> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT id, url, title, description, cv, status,
                review_score, review_feedback, verification, rank,
                review_notes, created_at, company, progress
         FROM jobs WHERE id = ?",
    )
    .bind(job_id.to_string())
    .fetch_one(pool)
    .await?;
    Ok(JobRecord {
        id: job_id,
        url: row.try_get("url")?,
        title: row.try_get::<Option<String>, _>("title")?.unwrap_or_default(),
        description: row.try_get::<Option<String>, _>("description")?.unwrap_or_default(),
        cv: row.try_get::<Option<String>, _>("cv")?.unwrap_or_default(),
        status: row.try_get("status")?,
        company: row.try_get::<Option<String>, _>("company")?.unwrap_or_default(),
        review_score:    row.try_get("review_score")?,
        review_feedback: row.try_get("review_feedback")?,
        verification:    row.try_get("verification")?,
        rank:            row.try_get("rank")?,
        review_notes:    row.try_get("review_notes")?,
        created_at:      row.try_get("created_at")?,
        progress:        row.try_get::<Option<String>, _>("progress")?.unwrap_or_default(),
    })
}

/// Get all jobs for the dashboard list.
pub async fn list_jobs(pool: &SqlitePool) -> Result<Vec<JobListRow>> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, title, status, review_score, company, progress FROM jobs
         ORDER BY CASE status
           WHEN 'new' THEN 0
           WHEN 'generating' THEN 1
           WHEN 'scraping' THEN 2
           WHEN 'pre_screening' THEN 3
           ELSE 4
         END, rowid DESC"
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| {
            let id_str: String = r.try_get("id")?;
            let id = Uuid::parse_str(&id_str).map_err(|e| anyhow::anyhow!(e))?;
            Ok(JobListRow {
                id,
                title: r.try_get::<Option<String>, _>("title")?.unwrap_or_default(),
                status: r.try_get("status")?,
                score: r.try_get::<Option<i64>, _>("review_score").ok().flatten(),
                company: r.try_get::<Option<String>, _>("company")?.unwrap_or_default(),
                progress: r.try_get::<Option<String>, _>("progress")?.unwrap_or_default(),
            })
        })
        .collect()
}

/// Save review_notes without changing status (used before regenerate).
pub async fn save_review_notes(pool: &SqlitePool, job_id: Uuid, notes: &str) -> Result<()> {
    sqlx::query("UPDATE jobs SET review_notes = ? WHERE id = ?")
        .bind(notes)
        .bind(job_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Look up a job ID by URL; returns None if no such row exists.
/// Used by POST /jobs to detect duplicate URL submissions.
pub async fn get_job_id_by_url(pool: &SqlitePool, url: &str) -> Result<Option<Uuid>> {
    use sqlx::Row;
    let row = sqlx::query("SELECT id FROM jobs WHERE url = ?")
        .bind(url)
        .fetch_optional(pool)
        .await?;
    match row {
        None => Ok(None),
        Some(r) => {
            let id_str: String = r.try_get("id")?;
            let id = Uuid::parse_str(&id_str).map_err(|e| anyhow::anyhow!(e))?;
            Ok(Some(id))
        }
    }
}

pub async fn delete_job(pool: &SqlitePool, job_id: Uuid) -> Result<bool> {
    let result = sqlx::query("DELETE FROM jobs WHERE id = ?")
        .bind(job_id.to_string())
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_jobs(pool: &SqlitePool, ids: &[String]) -> Result<u64> {
    let mut tx = pool.begin().await?;
    let mut count: u64 = 0;
    for id in ids {
        let result = sqlx::query("DELETE FROM jobs WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        count += result.rows_affected();
    }
    tx.commit().await?;
    Ok(count)
}

// ---------------------------------------------------------------------------
// Search (batch crawl) helpers
// ---------------------------------------------------------------------------

pub async fn create_search(pool: &SqlitePool, id: Uuid, url: &str, found_count: i64) -> Result<()> {
    sqlx::query("INSERT INTO searches (id, url, found_count) VALUES (?, ?, ?)")
        .bind(&id.to_string())
        .bind(url)
        .bind(found_count)
        .execute(pool)
        .await?;
    Ok(())
}

/// Count jobs linked to a search in terminal vs. non-terminal states.
/// Returns (terminal_count, total_count).
pub async fn get_search_progress(pool: &SqlitePool, search_id: Uuid) -> Result<(i64, i64)> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT
            COUNT(*) AS total,
            SUM(CASE WHEN status IN ('generated')
                     THEN 1 ELSE 0 END) AS terminal
         FROM jobs WHERE search_id = ?"
    )
    .bind(&search_id.to_string())
    .fetch_one(pool)
    .await?;
    Ok((row.try_get("terminal")?, row.try_get("total")?))
}

/// Link an existing job to a search (used when a discovered URL already existed in the DB).
pub async fn link_job_to_search(pool: &SqlitePool, job_id: Uuid, search_id: Uuid) -> Result<()> {
    sqlx::query("UPDATE jobs SET search_id = ? WHERE id = ? AND search_id IS NULL")
        .bind(&search_id.to_string())
        .bind(&job_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Review/verification/rank save helpers
// ---------------------------------------------------------------------------

/// Save review score and feedback to the job record.
pub async fn save_review(
    pool: &SqlitePool,
    job_id: Uuid,
    score: i64,
    feedback: &str,
) -> Result<()> {
    sqlx::query("UPDATE jobs SET review_score = ?, review_feedback = ? WHERE id = ?")
        .bind(score)
        .bind(feedback)
        .bind(job_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Save verification JSON blob to the job record.
pub async fn save_verification(pool: &SqlitePool, job_id: Uuid, verification: &Value) -> Result<()> {
    sqlx::query("UPDATE jobs SET verification = ? WHERE id = ?")
        .bind(verification.to_string())
        .bind(job_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Save rank JSON blob to the job record.
pub async fn save_rank(pool: &SqlitePool, job_id: Uuid, rank: &Value) -> Result<()> {
    sqlx::query("UPDATE jobs SET rank = ? WHERE id = ?")
        .bind(rank.to_string())
        .bind(job_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Scheduler runs CRUD
// ---------------------------------------------------------------------------

pub async fn create_scheduler_run(pool: &SqlitePool, queries_run: i64) -> Result<i64> {
    use sqlx::Row;
    let row = sqlx::query(
        "INSERT INTO scheduler_runs (queries_run) VALUES (?) RETURNING id"
    )
    .bind(queries_run)
    .fetch_one(pool)
    .await?;
    Ok(row.try_get("id")?)
}

pub async fn finish_scheduler_run(
    pool: &SqlitePool,
    run_id: i64,
    errors: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE scheduler_runs SET finished_at = datetime('now'), status = 'completed', errors = ? WHERE id = ?"
    )
    .bind(errors)
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_scheduler_runs(pool: &SqlitePool, limit: i64) -> Result<Vec<SchedulerRunRow>> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT started_at, finished_at, status, queries_run, jobs_found, jobs_filtered
         FROM scheduler_runs ORDER BY id DESC LIMIT ?"
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| Ok(SchedulerRunRow {
            started_at:    r.try_get("started_at")?,
            finished_at:   r.try_get("finished_at")?,
            status:        r.try_get("status")?,
            queries_run:   r.try_get("queries_run")?,
            jobs_found:    r.try_get("jobs_found")?,
            jobs_filtered: r.try_get("jobs_filtered")?,
        }))
        .collect()
}

pub struct JobRecord {
    pub id:              Uuid,
    pub url:             String,
    pub title:           String,
    pub description:     String,
    pub cv:              String,
    pub status:          String,
    pub company:         String,
    pub review_score:    Option<i64>,
    pub review_feedback: Option<String>,
    pub verification:    Option<String>,
    pub rank:            Option<String>,
    pub review_notes:    Option<String>,
    pub created_at:      Option<String>,
    pub progress:        String,
}

pub struct JobListRow {
    pub id:       Uuid,
    pub title:    String,
    pub status:   String,
    pub score:    Option<i64>,
    pub company:  String,
    pub progress: String,
}

pub struct SchedulerRunRow {
    pub started_at:    String,
    pub finished_at:   Option<String>,
    pub status:        String,
    pub queries_run:   i64,
    pub jobs_found:    i64,
    pub jobs_filtered: i64,
}

// ---------------------------------------------------------------------------
// Workshop / manual job helpers
// ---------------------------------------------------------------------------

/// Create a manual job stub with title, company, and description pre-filled.
/// Uses a synthetic URL (`"manual: {title}"`) so it won't collide with scraped jobs.
pub async fn create_manual_job_stub(
    pool: &SqlitePool,
    title: &str,
    company: &str,
    description: &str,
    _source_url: &str,
) -> Result<Uuid> {
    let id = Uuid::new_v4();
    let id_str = id.to_string();
    let url = format!("manual:{id_str}");
    sqlx::query(
        "INSERT INTO jobs (id, url, title, description, company, status, created_at)
         VALUES (?, ?, ?, ?, ?, 'new', datetime('now'))"
    )
    .bind(&id_str)
    .bind(&url)
    .bind(title)
    .bind(description)
    .bind(company)
    .execute(pool)
    .await?;
    Ok(id)
}

/// A workshop job row includes parsed pipeline outputs for inline rendering.
pub struct WorkshopJobRow {
    pub id:                  Uuid,
    pub url:                 String,
    pub title:               String,
    pub company:             String,
    pub status:              String,
    pub progress:            String,
    pub review_score:        Option<i64>,
    pub review_feedback:     Option<String>,
    pub verification:        Option<String>,
    pub rank:                Option<String>,
}

/// List manual jobs (url LIKE 'manual:%') for the workshop page.
pub async fn list_workshop_jobs(pool: &SqlitePool) -> Result<Vec<WorkshopJobRow>> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, url, title, company, status, progress,
                review_score, review_feedback, verification, rank
         FROM jobs
         WHERE url LIKE 'manual:%'
         ORDER BY rowid DESC"
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| {
            let id_str: String = r.try_get("id")?;
            let id = Uuid::parse_str(&id_str).map_err(|e| anyhow::anyhow!(e))?;
            Ok(WorkshopJobRow {
                id,
                url:             r.try_get::<Option<String>, _>("url")?.unwrap_or_default(),
                title:           r.try_get::<Option<String>, _>("title")?.unwrap_or_default(),
                company:         r.try_get::<Option<String>, _>("company")?.unwrap_or_default(),
                status:          r.try_get("status")?,
                progress:        r.try_get::<Option<String>, _>("progress")?.unwrap_or_default(),
                review_score:    r.try_get("review_score")?,
                review_feedback: r.try_get("review_feedback")?,
                verification:    r.try_get("verification")?,
                rank:            r.try_get("rank")?,
            })
        })
        .collect()
}
