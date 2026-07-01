use std::sync::atomic::Ordering;
use axum::{
    extract::{Form, Path, State},
    response::{Html, IntoResponse, Response},
};
use axum::body::Bytes;
use uuid::Uuid;
use askama::Template;
use crate::{
    AppState, db, generate, events, profile,
    templates::{
        CvPrintTemplate, IndexTemplate, JobRow, JobTemplate, JobDetailFragment,
        ProcessingTemplate, WorkshopProcessingCard, ReviewSummary,
        Verification, VerificationItem, RankSummary,
        CrawlStatusTemplate, JobListTemplate, parse_cv_content,
    },
};
use super::forms::*;

// ponytail: hardcoded host allowlist; replace with config-driven list in Phase 2
pub(crate) fn is_jobstreet_url(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h == "id.jobstreet.com"))
        .unwrap_or(false)
}

pub async fn index(State(app): State<AppState>) -> impl IntoResponse {
    let rows = db::list_jobs(&app.db).await.unwrap_or_default();
    let jobs = rows
        .into_iter()
        .map(|r| JobRow { id: r.id, title: r.title, status: r.status, score: r.score.unwrap_or(0), company: r.company, progress: r.progress })
        .collect();
    let activity = app.crawl_activity.read().map(|a| a.clone()).unwrap_or_default();
    let active = activity.active;
    let stopping = activity.stopping;
    let message = activity.message.clone();
    let (terminal, total) = match activity.search_id {
        Some(sid) if active => db::get_search_progress(&app.db, sid).await.unwrap_or((0, 0)),
        _ => (0, 0),
    };
    let crawl_html = CrawlStatusTemplate { active, stopping, message, terminal, total }
        .render().unwrap_or_default();
    IndexTemplate { jobs, crawl_html }
}

// GET /jobs/list — polled by #job-list so new entries appear without a full
// page reload. Returns just the inner fragment; the div keeps its hx-trigger.
pub async fn job_list(State(app): State<AppState>) -> Response {
    let rows = db::list_jobs(&app.db).await.unwrap_or_default();
    let jobs = rows
        .into_iter()
        .map(|r| JobRow { id: r.id, title: r.title, status: r.status, score: r.score.unwrap_or(0), company: r.company, progress: r.progress })
        .collect();
    JobListTemplate { jobs }.into_response()
}

// POST /jobs — create stub record, spawn background task, return polling card immediately.
pub async fn submit_job(
    State(app): State<AppState>,
    Form(body): Form<JobForm>,
) -> Response {
    if !is_jobstreet_url(&body.url) {
        return Html(
            "<article><span class=\"error\">Only id.jobstreet.com URLs are supported.</span></article>"
                .to_string(),
        )
        .into_response();
    }

    if let Ok(Some(existing_id)) = db::get_job_id_by_url(&app.db, &body.url).await {
        let url = db::get_job_url(&app.db, existing_id)
            .await
            .unwrap_or_else(|_| body.url.clone());
        return ProcessingTemplate { id: existing_id, url, progress: String::new() }.into_response();
    }

    let job_id = match db::create_job_stub(&app.db, &body.url).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("create_job_stub failed: {e}");
            return Html(format!(
                "<article><span class=\"error\">Failed to create job: {e}</span></article>"
            ))
            .into_response();
        }
    };

    tokio::spawn({
        let app = app.clone();
        async move {
            if let Err(e) = generate::process_job(&app, job_id).await {
                eprintln!("process_job {job_id} failed: {e}");
                let _ = db::delete_job(&app.db, job_id).await;
            }
        }
    });

    ProcessingTemplate { id: job_id, url: body.url, progress: String::new() }.into_response()
}

// GET /crawl/status — polled by the status panel on the main page.
// Returns the panel HTML for the current global crawl state.
pub async fn crawl_status(State(app): State<AppState>) -> Response {
    let activity = app
        .crawl_activity
        .read()
        .map(|a| a.clone())
        .unwrap_or_default();
    let active = activity.active;
    let stopping = activity.stopping;
    let message = activity.message.clone();
    let (terminal, total) = match activity.search_id {
        Some(sid) if active => db::get_search_progress(&app.db, sid)
            .await
            .unwrap_or((0, 0)),
        _ => (0, 0),
    };
    CrawlStatusTemplate { active, stopping, message, terminal, total }.into_response()
}

// POST /crawl/stop — set the cancel flag. The in-flight job finishes, then the
// crawler loop bails on the next iteration.
pub async fn crawl_stop(State(app): State<AppState>) -> Response {
    app.crawl_cancel.store(true, Ordering::Relaxed);
    if let Ok(mut a) = app.crawl_activity.write() {
        a.stopping = true;
    }
    // Re-render the panel so the Stop button swaps to "wait…" immediately.
    crawl_status(State(app)).await
}

pub async fn job_card(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Response {
    let status = db::get_status(&app.db, job_id)
        .await
        .unwrap_or_default();

    match status.as_str() {
        "generated" => {
            db::render_cv_ready(&app.db, job_id).await.into_response()
        }
        "failed" => Html(format!(
            "<article id=\"job-{job_id}\"><span class=\"error\">Processing failed.</span></article>"
        ))
        .into_response(),
        "" => {
            // Row missing (deleted mid-pipeline) — return terminal card to stop polling.
            Html(format!(
                "<article id=\"job-{job_id}\"><span class=\"error\">Job removed.</span></article>"
            )).into_response()
        }
        _ => {
            let (url, progress) = db::get_job_card_data(&app.db, job_id)
                .await
                .unwrap_or_default();
            ProcessingTemplate { id: job_id, url, progress }.into_response()
        }
    }
}

// GET /jobs/:id — CV review page (full page with navbar)
pub async fn job_detail(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Response {
    render_job_detail(&app, job_id, false).await
}

// GET /jobs/:id/fragment — job detail fragment only (no navbar, for SSE/HTMX swaps)
pub async fn job_detail_fragment(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Response {
    render_job_detail(&app, job_id, true).await
}

// Shared render logic for the job detail page. Used by job_detail,
// job_detail_fragment, and regenerate_job.
async fn render_job_detail(app: &AppState, job_id: Uuid, fragment: bool) -> Response {
    let rec = match db::get_job(&app.db, job_id).await {
        Ok(r) => r,
        Err(e) => {
            return Html(format!("<p>Error loading job: {}</p>",
                e.to_string().replace('&', "&amp;").replace('<', "&lt;"))).into_response();
        }
    };

    let cv = parse_cv_content(&rec.cv);

    let review = rec.review_feedback.as_deref().map(|feedback| {
        ReviewSummary {
            score: rec.review_score.unwrap_or(0),
            feedback: feedback.to_string(),
        }
    });

    let verification = rec.verification.as_deref().and_then(|s| {
        serde_json::from_str::<serde_json::Value>(s).ok().map(|v| {
            let items = v["items"].as_array().map(|arr| {
                arr.iter().map(|it| VerificationItem {
                    category: it["category"].as_str().unwrap_or("").to_string(),
                    field:    it["field"].as_str().unwrap_or("").to_string(),
                    claim:    it["claim"].as_str().unwrap_or("").to_string(),
                    verdict:  it["verdict"].as_str().unwrap_or("").to_string(),
                    evidence: it["evidence"].as_str().unwrap_or("").to_string(),
                }).collect()
            }).unwrap_or_default();

            Verification {
                truth_pct: v["truth_pct"].as_i64().unwrap_or(0),
                items,
                gap_report: v["gap_report"].as_str().unwrap_or("").to_string(),
                fabrication_detected: v["fabrication_detected"].as_bool().unwrap_or(false),
                incomplete: v["incomplete"].as_bool().unwrap_or(false),
            }
        })
    });

    let rank = rec.rank.as_deref().and_then(|s| {
        serde_json::from_str::<serde_json::Value>(s).ok().map(|v| {
            RankSummary {
                approval_probability: v["approval_probability"].as_i64().unwrap_or(0),
                good: v["good"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default(),
                bad: v["bad"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default(),
                improvements: v["improvements"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default(),
            }
        })
    });

    let review_notes = rec.review_notes.unwrap_or_default();

    if fragment {
        JobDetailFragment {
            id: job_id, title: rec.title, url: rec.url, company: rec.company,
            description: rec.description, cv, status: rec.status, progress: rec.progress,
            review, verification, rank, review_notes,
        }.into_response()
    } else {
        JobTemplate {
            id: job_id, title: rec.title, url: rec.url, company: rec.company,
            description: rec.description, cv, status: rec.status, progress: rec.progress,
            review, verification, rank, review_notes,
        }.into_response()
    }
}

// GET /jobs/:id/cv — print-optimized standalone CV page (browser print → PDF)
pub async fn cv_print(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Response {
    let rec = match db::get_job(&app.db, job_id).await {
        Ok(r) => r,
        Err(e) => {
            return Html(format!("<p>Error loading job: {}</p>",
                e.to_string().replace('&', "&amp;").replace('<', "&lt;"))).into_response();
        }
    };

    let cv = parse_cv_content(&rec.cv);

    let (name, title) = match db::get_master_cv(&app.db).await {
        Ok(master) => profile::extract_name_title(&master),
        Err(_) => (String::new(), String::new()),
    };

    CvPrintTemplate { name, title, summary: cv.summary, skills: cv.skills, experiences: cv.experiences }
    .into_response()
}

pub async fn delete_job(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Response {
    let _ = db::delete_job(&app.db, job_id).await;
    axum::response::Html("").into_response()
}

pub async fn delete_batch(
    State(app): State<AppState>,
    body: Bytes,
) -> Response {
    // ponytail: manual parse — serde_urlencoded rejects repeated keys for Vec<T>.
    let ids: Vec<String> = form_urlencoded::parse(&body)
        .filter(|(k, _)| k == "ids")
        .map(|(_, v)| v.into_owned())
        .collect();
    let _ = db::delete_jobs(&app.db, &ids).await;
    // Return the refreshed fragment instead of HX-Refresh — no full page flash.
    job_list(State(app)).await
}

// POST /jobs/regenerate-batch — re-run the full pipeline for multiple jobs.
// Same multi-select pattern as delete_batch: parse repeated `ids` form keys.
pub async fn regenerate_batch(
    State(app): State<AppState>,
    body: Bytes,
) -> Response {
    let ids: Vec<String> = form_urlencoded::parse(&body)
        .filter(|(k, _)| k == "ids")
        .map(|(_, v)| v.into_owned())
        .collect();
    for id_str in &ids {
        if let Ok(id) = Uuid::parse_str(id_str) {
            let app = app.clone();
            tokio::spawn(async move {
                if let Err(e) = generate::process_job(&app, id).await {
                    eprintln!("regenerate_batch {id} failed: {e}");
                }
            });
        }
    }
    // Return the refreshed fragment — jobs are processing in background.
    job_list(State(app)).await
}

pub async fn regenerate_job(
    State(app): State<AppState>,
    Path(job_id): Path<Uuid>,
    Form(body): Form<RegenerateForm>,
) -> Response {
    let feedback = body.review_notes.as_deref().unwrap_or("");
    let _ = db::save_review_notes(&app.db, job_id, feedback).await;
    db::set_status(&app.db, job_id, "generating").await.ok();
    db::set_progress(&app.db, job_id, "Regenerating…").await.ok();
    events::publish_job_update(&app, job_id, "generating", "Regenerating…");

    if body.full_pipeline.as_deref() == Some("true") {
        // Full pipeline re-run (pre-screen + writer + review loop + verifier + editor + ranker)
        tokio::spawn({
            let app = app.clone();
            async move {
                if let Err(e) = generate::process_manual_job(&app, job_id).await {
                    eprintln!("regenerate full_pipeline {job_id} failed: {e}");
                    let _ = db::set_status(&app.db, job_id, "failed").await;
                }
            }
        });
    } else {
        // Simple regenerate (writer-only)
        let feedback_owned = feedback.to_string();
        tokio::spawn({
            let app = app.clone();
            async move {
                if let Err(e) = generate::regenerate_cv(&app, job_id, &feedback_owned).await {
                    eprintln!("regenerate {job_id} failed: {e}");
                    let _ = db::set_status(&app.db, job_id, "failed").await;
                }
            }
        });
    }

    // Re-render the job detail fragment with updated status/progress.
    // The outer SSE div in job.html will auto-update on each pipeline stage.
    render_job_detail(&app, job_id, true).await
}

// POST /jobs/manual — create a manual job (no scraping), spawn full pipeline.
pub async fn submit_manual_job(
    State(app): State<AppState>,
    Form(body): Form<ManualJobForm>,
) -> Response {
    let title = body.title.trim().to_string();
    let description = body.description.trim().to_string();
    if title.is_empty() || description.is_empty() {
        return Html(
            "<article><span class=\"error\">Title and description are required.</span></article>"
                .to_string(),
        )
        .into_response();
    }
    let company = body.company.unwrap_or_default();
    let source_url = body.source_url.unwrap_or_default();

    let job_id = match db::create_manual_job_stub(&app.db, &title, &company, &description, &source_url).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("create_manual_job_stub failed: {e}");
            return Html(format!(
                "<article><span class=\"error\">Failed to create job: {e}</span></article>"
            ))
            .into_response();
        }
    };

    tokio::spawn({
        let app = app.clone();
        async move {
            if let Err(e) = generate::process_manual_job(&app, job_id).await {
                eprintln!("process_manual_job {job_id} failed: {e}");
                let _ = db::delete_job(&app.db, job_id).await;
            }
        }
    });

    WorkshopProcessingCard {
        id: job_id,
        title,
        company,
        progress: String::new(),
    }
    .into_response()
}

