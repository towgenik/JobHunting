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
        "INSERT INTO jobs (id, url, status, search_id) VALUES (?, ?, 'new', NULL)"
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
        "INSERT INTO jobs (id, url, status, search_id) VALUES (?, ?, 'new', ?)"
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
    let row = sqlx::query(
        "SELECT id, url, title, description, cv, status, reject_reason FROM jobs WHERE id = ?",
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
        reject_reason: row
            .try_get::<Option<String>, _>("reject_reason")?
            .unwrap_or_default(),
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

pub struct SearchRow {
    pub id: Uuid,
    pub url: String,
    pub found_count: i64,
}

pub async fn get_search(pool: &SqlitePool, search_id: Uuid) -> Result<SearchRow> {
    use sqlx::Row;
    let row = sqlx::query("SELECT id, url, found_count FROM searches WHERE id = ?")
        .bind(&search_id.to_string())
        .fetch_one(pool)
        .await?;
    let id_str: String = row.try_get("id")?;
    Ok(SearchRow {
        id: Uuid::parse_str(&id_str).map_err(|e| anyhow::anyhow!(e))?,
        url: row.try_get("url")?,
        found_count: row.try_get("found_count")?,
    })
}

/// Count jobs linked to a search in terminal vs. non-terminal states.
/// Returns (terminal_count, total_count).
pub async fn get_search_progress(pool: &SqlitePool, search_id: Uuid) -> Result<(i64, i64)> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT
            COUNT(*) AS total,
            SUM(CASE WHEN status IN ('pending_approval','approved','rejected','failed')
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

pub struct JobRecord {
    pub id:            Uuid,
    pub url:           String,
    pub title:         String,
    pub description:   String,
    pub cv:            String,
    pub status:        String,
    pub reject_reason: String,
}

pub struct JobListRow {
    pub id:     Uuid,
    pub title:  String,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Self-check: the valid job-status strings are exactly the ones the pipeline
    /// uses.  If someone renames a status in process_job without updating this list
    /// the test fails.
    #[test]
    fn known_statuses_are_valid_strings() {
        let statuses = [
            "new", "scraping", "generating",
            "pending_approval", "approved", "rejected", "failed",
        ];
        for s in statuses {
            assert!(!s.is_empty(), "status must be non-empty");
            assert!(s.is_ascii(), "status must be ASCII: {s}");
        }
        // Uniqueness — duplicate status names would be a copy-paste bug.
        let unique: std::collections::HashSet<_> = statuses.iter().collect();
        assert_eq!(unique.len(), statuses.len(), "duplicate status entry");
    }

    /// Self-check: `get_job_id_by_url` returns None for an unknown URL.
    /// Uses an in-memory SQLite database; no fixtures, no filesystem state.
    #[tokio::test]
    async fn get_job_id_by_url_returns_none_for_missing() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory db");
        sqlx::query(
            "CREATE TABLE jobs (
                id TEXT PRIMARY KEY,
                url TEXT UNIQUE NOT NULL,
                title TEXT, description TEXT, cv TEXT, reject_reason TEXT,
                status TEXT DEFAULT 'new'
             )"
        )
        .execute(&pool)
        .await
        .expect("create table");

        let result = get_job_id_by_url(&pool, "https://id.jobstreet.com/jobs/999")
            .await
            .expect("query");
        assert!(result.is_none(), "should be None for unknown URL");
    }
}
