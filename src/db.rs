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
        "INSERT INTO jobs (id, url, status) VALUES (?, ?, 'new')"
    )
    .bind(&id_str)
    .bind(url)
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
    sqlx::query("UPDATE jobs SET title = ?, description = ? WHERE id = ?")
        .bind(&title)
        .bind(&description)
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
    let row = sqlx::query("SELECT id, url, title, description, cv, status FROM jobs WHERE id = ?")
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
    })
}

/// Get all jobs for the dashboard list.
pub async fn list_jobs(pool: &SqlitePool) -> Result<Vec<JobListRow>> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, title, status FROM jobs ORDER BY rowid DESC"
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
            })
        })
        .collect()
}

/// Set reject_reason and status to 'rejected'.
pub async fn reject_job(pool: &SqlitePool, job_id: Uuid, reason: &str) -> Result<()> {
    sqlx::query("UPDATE jobs SET status = 'rejected', reject_reason = ? WHERE id = ?")
        .bind(reason)
        .bind(job_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Set status to 'approved'.
pub async fn approve_job(pool: &SqlitePool, job_id: Uuid) -> Result<()> {
    sqlx::query("UPDATE jobs SET status = 'approved' WHERE id = ?")
        .bind(job_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub struct JobRecord {
    pub id:          Uuid,
    pub url:         String,
    pub title:       String,
    pub description: String,
    pub cv:          String,
    pub status:      String,
}

pub struct JobListRow {
    pub id:     Uuid,
    pub title:  String,
    pub status: String,
}
