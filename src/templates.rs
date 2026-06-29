use askama::Template;
use uuid::Uuid;
use crate::db::SchedulerRunRow;
use crate::profile::ProfileFile;

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub jobs:      Vec<JobRow>,
    pub crawl_html: String,
}

pub struct JobRow {
    pub id:     Uuid,
    pub title:  String,
    pub status: String,
    pub score:  i64,
    pub company: String,
}

#[derive(Template)]
#[template(path = "job.html")]
pub struct JobTemplate {
    pub id:              Uuid,
    pub title:           String,
    #[allow(dead_code)]
    pub url:             String,
    pub company:         String,
    pub description:     String,
    #[allow(dead_code)]
    pub cv:              CvContent,
    pub status:          String,
    pub review:          Option<ReviewSummary>,
    pub verification:    Option<Verification>,
    pub rank:            Option<RankSummary>,
    pub review_notes:    String,
}

pub struct CvContent {
    #[allow(dead_code)]
    pub summary:     String,
    #[allow(dead_code)]
    pub skills:      Vec<String>,
    #[allow(dead_code)]
    pub experiences: Vec<Experience>,
}

pub struct Experience {
    pub company:       String,
    pub role:          String,
    pub bullet_points: Vec<String>,
}

/// Parse CV JSON string into a CvContent. Used by both job_detail and cv_print.
pub fn parse_cv_content(cv_json: &str) -> CvContent {
    let cv_val: serde_json::Value = serde_json::from_str(cv_json).unwrap_or_default();
    let summary = cv_val["summary"].as_str().unwrap_or("").to_string();
    let skills: Vec<String> = cv_val["skills"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let experiences: Vec<Experience> = cv_val["experiences"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|e| Experience {
                    company: e["company"].as_str().unwrap_or("").to_string(),
                    role: e["role"].as_str().unwrap_or("").to_string(),
                    bullet_points: e["bullet_points"]
                        .as_array()
                        .map(|bp| {
                            bp.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    CvContent { summary, skills, experiences }
}

pub struct ReviewSummary {
    pub score:    i64,
    pub feedback: String,
}

pub struct Verification {
    pub truth_pct:             i64,
    pub items:                 Vec<VerificationItem>,
    pub gap_report:            String,
    pub fabrication_detected:  bool,
    pub incomplete:            bool,
}

pub struct VerificationItem {
    pub category: String,
    pub field:    String,
    pub claim:    String,
    pub verdict:  String,
    pub evidence: String,
}

pub struct RankSummary {
    pub approval_probability: i64,
    pub good:                 Vec<String>,
    pub bad:                  Vec<String>,
    pub improvements:         Vec<String>,
}

#[derive(Template)]
#[template(path = "fragments/processing.html")]
pub struct ProcessingTemplate {
    pub id:       Uuid,
    pub url:      String,
    pub progress: String,
}

#[derive(Template)]
#[template(path = "fragments/cv_ready.html")]
pub struct CvReadyTemplate {
    pub id:    Uuid,
    pub title: String,
}

#[derive(Template)]
#[template(path = "fragments/crawl_status.html")]
pub struct CrawlStatusTemplate {
    pub active:   bool,        // a crawl is currently running
    pub stopping: bool,        // user pressed stop, finishing current job
    pub message:  String,      // human-readable activity line
    pub terminal: i64,         // jobs in terminal state for the active search
    pub total:    i64,         // total jobs in the active search
}

#[derive(Template)]
#[template(path = "fragments/job_list.html")]
pub struct JobListTemplate {
    pub jobs: Vec<JobRow>,
}

#[derive(Template)]
#[template(path = "settings.html")]
pub struct SettingsTemplate {
    pub llm_endpoint:     String,
    pub llm_api_key:      String,
    pub llm_model:        String,
    pub llm_openai_compat: bool,
    pub llm_mock:         bool,
    pub scheduler_interval: i64,
    pub scheduler_date_range: i64,
    pub scheduler_max_pages: i64,
    pub scheduler_runs:   Vec<SchedulerRunRow>,
    pub status:           String,
    // Agent settings
    pub agent_ctx_window:          i64,
    pub agent_max_output:          i64,
    pub agent_thinking_effort:     String,
    pub agent_wiki_query_max_hops: i64,
    pub wiki_auto_ingest:          bool,
    // Pipeline tuning
    pub llm_concurrency:           i64,
    pub max_jobs_per_crawl:        i64,
}

#[derive(Template)]
#[template(path = "profile.html")]
pub struct ProfileTemplate {
    pub files:        Vec<ProfileFile>,
    pub current_file: String,
    pub content:      String,
}

#[derive(Template)]
#[template(path = "cv_print.html")]
pub struct CvPrintTemplate {
    pub name:         String,
    pub title:        String,
    pub summary:      String,
    pub skills:       Vec<String>,
    pub experiences:  Vec<Experience>,
}

