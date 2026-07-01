use axum::{
    extract::State,
    response::{IntoResponse, Response},
};
use serde_json::Value;
use crate::{
    AppState, db,
    templates::{WorkshopTemplate, WorkshopListTemplate, WorkshopJob},
};

fn build_workshop_jobs(rows: &[db::WorkshopJobRow]) -> Vec<WorkshopJob> {
    rows.iter().map(|r| {
        let (truth_pct, fabrication_detected, fabrication_items) =
            r.verification.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok())
                .map(|v| {
                    let items = v["items"].as_array()
                        .map(|arr| arr.iter()
                            .filter(|it| it["verdict"].as_str() == Some("lie"))
                            .filter_map(|it| it["claim"].as_str().map(|s| s.to_string()))
                            .collect())
                        .unwrap_or_default();
                    (
                        v["truth_pct"].as_i64().unwrap_or(0),
                        v["fabrication_detected"].as_bool().unwrap_or(false),
                        items,
                    )
                })
                .unwrap_or((0, false, vec![]));

        let (approval_probability, improvements) =
            r.rank.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok())
                .map(|v| {
                    let imp = v["improvements"].as_array()
                        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default();
                    (v["approval_probability"].as_i64().unwrap_or(0), imp)
                })
                .unwrap_or((0, vec![]));

        WorkshopJob {
            id: r.id,
            url: r.url.clone(),
            title: r.title.clone(),
            company: r.company.clone(),
            status: r.status.clone(),
            progress: r.progress.clone(),
            review_score: r.review_score.unwrap_or(0),
            review_feedback: r.review_feedback.clone().unwrap_or_default(),
            truth_pct,
            fabrication_detected,
            approval_probability,
            improvements,
            fabrication_items,
        }
    }).collect()
}

pub async fn workshop_page(State(app): State<AppState>) -> Response {
    let rows = db::list_workshop_jobs(&app.db).await.unwrap_or_default();
    let jobs = build_workshop_jobs(&rows);
    WorkshopTemplate { jobs }.into_response()
}

pub async fn workshop_list(State(app): State<AppState>) -> Response {
    let rows = db::list_workshop_jobs(&app.db).await.unwrap_or_default();
    let jobs = build_workshop_jobs(&rows);
    WorkshopListTemplate { jobs }.into_response()
}
