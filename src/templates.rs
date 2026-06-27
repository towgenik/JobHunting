use askama::Template;
use uuid::Uuid;
use crate::db::{SearchQueryRow, SchedulerRunRow};

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub jobs: Vec<JobRow>,
}

pub struct JobRow {
    pub id:     Uuid,
    pub title:  String,
    pub status: String,
}

#[derive(Template)]
#[template(path = "job.html")]
pub struct JobTemplate {
    pub id:              Uuid,
    pub title:           String,
    pub description:     String,
    pub cv:              CvContent,
    pub status:          String,
    pub reject_reason:   String,
    pub review:          Option<ReviewSummary>,
    pub verification:    Option<Verification>,
    pub rank:            Option<RankSummary>,
    pub review_notes:    String,
}

pub struct CvContent {
    pub summary:     String,
    pub skills:      Vec<String>,
    pub experiences: Vec<Experience>,
}

pub struct Experience {
    pub company:       String,
    pub role:          String,
    pub bullet_points: Vec<String>,
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
    pub id:  Uuid,
    pub url: String,
}

#[derive(Template)]
#[template(path = "fragments/cv_ready.html")]
pub struct CvReadyTemplate {
    pub id:    Uuid,
    pub title: String,
}

#[derive(Template)]
#[template(path = "fragments/search_card.html")]
pub struct SearchCardTemplate {
    pub search_id: Uuid,
    pub url:       String,
    pub terminal:  i64,
    pub total:     i64,
}

#[derive(Template)]
#[template(path = "settings.html")]
pub struct SettingsTemplate {
    pub master_cv:       String,
    pub search_queries:  Vec<SearchQueryRow>,
    pub recent_feedback: Vec<String>,
    pub scheduler_runs:  Vec<SchedulerRunRow>,
    pub status:          String,
}

#[derive(Template)]
#[template(path = "fragments/search_queries.html")]
pub struct SearchQueriesTemplate {
    pub search_queries: Vec<SearchQueryRow>,
    pub status:         String,
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

