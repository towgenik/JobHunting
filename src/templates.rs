use askama::Template;
use uuid::Uuid;

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
    pub id:            Uuid,
    pub title:         String,
    pub description:   String,
    pub cv:            CvContent,
    pub status:        String,
    pub reject_reason: String,
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
#[template(path = "settings.html")]
pub struct SettingsTemplate {
    pub master_cv: String,
}
