//! Database module — sqlx queries for jobs, settings, and scheduler.

pub mod jobs;
pub mod settings;

pub use jobs::*;
pub use settings::*;

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    #[test]
    fn known_statuses_are_valid_strings() {
        let statuses = [
            "new", "scraping", "pre_screening", "generating", "generated", "failed",
        ];
        for s in statuses {
            assert!(!s.is_empty(), "status must be non-empty");
            assert!(s.is_ascii(), "status must be ASCII: {s}");
        }
        let unique: std::collections::HashSet<_> = statuses.iter().collect();
        assert_eq!(unique.len(), statuses.len(), "duplicate status entry");
    }

    #[tokio::test]
    async fn get_job_id_by_url_returns_none_for_missing() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory db");
        sqlx::query(
            "CREATE TABLE jobs (
                id TEXT PRIMARY KEY,
                url TEXT UNIQUE NOT NULL,
                title TEXT, description TEXT, cv TEXT,
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

    #[tokio::test]
    async fn scheduler_runs_crud() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory db");
        sqlx::query(
            "CREATE TABLE scheduler_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                started_at TEXT NOT NULL DEFAULT (datetime('now')),
                finished_at TEXT,
                status TEXT NOT NULL DEFAULT 'running',
                queries_run INTEGER NOT NULL DEFAULT 0,
                jobs_found INTEGER NOT NULL DEFAULT 0,
                jobs_filtered INTEGER NOT NULL DEFAULT 0,
                errors TEXT
             )"
        )
        .execute(&pool)
        .await
        .expect("create table");

        let run_id = create_scheduler_run(&pool, 5).await.unwrap();
        assert!(run_id > 0, "run_id should be positive");

        finish_scheduler_run(&pool, run_id, None).await.unwrap();

        let runs = list_scheduler_runs(&pool, 10).await.unwrap();
        assert_eq!(runs.len(), 1, "should have 1 run");
        assert_eq!(runs[0].queries_run, 5);
        assert_eq!(runs[0].status, "completed");
    }
}
