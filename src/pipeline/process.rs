use anyhow::Result;
use serde_json::json;
use uuid::Uuid;
use crate::{db, AppState};
use crate::events::publish_job_update;
use super::helpers::{strip_cv_metadata, parse_satisfied};
use super::pre_screen::pre_screen;
use super::writer::writer_call;
use super::reviewer::review_call;
use super::verifier::verify_call;
use super::editor::editor_call;
use super::ranker::ranker_call;
use super::context::build_wiki_context;

// ---------------------------------------------------------------------------
// Shared pipeline: deal-breakers → pre-screen → writer → review loop →
// verifier → editor → ranker → save
// ---------------------------------------------------------------------------

/// Bail out immediately if the user pressed Stop Crawl.
fn check_cancelled(app: &AppState) -> Result<()> {
    use std::sync::atomic::Ordering;
    if app.crawl_cancel.load(Ordering::Relaxed) {
        Err(anyhow::anyhow!("cancelled"))
    } else {
        Ok(())
    }
}

async fn run_pipeline(app: &AppState, job_id: Uuid, master_cv: &str, jd: &str) -> Result<()> {
    let agent = db::get_agent_settings(&app.db).await.unwrap_or_default();
    let max_output = agent.max_output.max(256) as u32;
    let thinking_effort = agent.thinking_effort.clone();
    let ctx_window = agent.ctx_window.max(1000) as u32;
    let max_iters = agent.max_review_iterations.max(1) as u32;
    // Per-agent overrides: override ?? global default (NULL = inherit).
    let overrides = db::get_agent_overrides(&app.db).await.unwrap_or_default();
    let eff = |role: &str| -> (u32, String) {
        let o = overrides.get(role);
        let mo = o.and_then(|x| x.max_output).map(|v| v.max(256) as u32).unwrap_or(max_output);
        let te = o.and_then(|x| x.thinking_effort.clone()).unwrap_or_else(|| thinking_effort.clone());
        (mo, te)
    };
    let (ps_out, ps_eff) = eff("prescreen");
    let (wr_out, wr_eff) = eff("writer");
    let (rv_out, rv_eff) = eff("reviewer");
    let (vf_out, vf_eff) = eff("verifier");
    let (ed_out, ed_eff) = eff("editor");
    let (rk_out, rk_eff) = eff("ranker");

    check_cancelled(app)?;

    // Deal-breaker scan
    let jd_lower = jd.to_lowercase();
    for kw in &app.profile_deal_breaker_keywords {
        if jd_lower.contains(&kw.to_lowercase()) {
            db::delete_job(&app.db, job_id).await?;
            eprintln!("job {job_id}: deal-breaker: {kw}");
            return Ok(());
        }
    }

    check_cancelled(app)?;

    // Pre-screen: early exit if this job is a poor fit.
    db::set_status(&app.db, job_id, "pre_screening").await?;
    let job_title = db::get_job(&app.db, job_id).await
        .map(|r| r.title)
        .unwrap_or_default();
    let prescreen_cv = {
        let wiki = app.wiki.read().unwrap_or_else(|e| e.into_inner());
        wiki.as_ref()
            .filter(|g| !g.is_empty())
            .map(|g| g.index_body().to_string())
            .unwrap_or_else(|| master_cv.to_string())
    };
    let (pre_score, pre_category) = pre_screen(app, &prescreen_cv, &job_title, jd, ps_out, Some(&ps_eff), Some(job_id)).await?;
    eprintln!("job {job_id}: pre-screen score={pre_score} category={pre_category}");
    if pre_score < 50 {
        eprintln!("job {job_id}: skipped — score {pre_score} < 50, category={pre_category}");
        publish_job_update(app, job_id, "skipped", &format!("score={pre_score}, {pre_category}"));
        let _ = db::delete_job(&app.db, job_id).await;
        return Ok(());
    }

    db::set_status(&app.db, job_id, "generating").await?;
    db::set_progress(&app.db, job_id, "Writer: drafting tailored CV…").await?;
    publish_job_update(app, job_id, "generating", "Writer: drafting tailored CV…");

    // 1. Initial draft
    eprintln!("job {job_id}: writer →");
    check_cancelled(app)?;
    let t0 = std::time::Instant::now();
    let wiki_context = build_wiki_context(app, jd, master_cv, ctx_window);
    let mut cv = writer_call(app, &wiki_context, jd, None, wr_out, Some(&wr_eff)).await?;
    eprintln!("job {job_id}: writer ✓ ({:.1}s)", t0.elapsed().as_secs_f64());
    let mut writer_constraints = cv.get("constraints").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string());
    strip_cv_metadata(&mut cv);
    db::save_cv_draft(&app.db, job_id, cv.clone()).await?;
    db::set_progress(&app.db, job_id, "Reviewer 1/{max_iters}: scoring draft…").await?;
    publish_job_update(app, job_id, "generating", "Reviewer 1/{max_iters}: scoring draft…");

    // 2. Review loop
    let mut review_score: i64 = 0;
    let mut review_feedback = String::new();
    let mut fabrication_warning = false;
    let mut satisfied = false;
    let mut iters_run: u32 = 0;
    let mut prev_score: i64 = 0;
    let mut plateau_count: u32 = 0;

    for iteration in 0..max_iters {
        iters_run = iteration + 1;
        check_cancelled(app)?;
        let t0 = std::time::Instant::now();
        eprintln!("job {job_id}: reviewer {iters_run}/{max_iters} →");
        let review = review_call(app, jd, &cv, writer_constraints.as_deref(), rv_out, Some(&rv_eff)).await?;
        review_score = review["score"].as_i64().unwrap_or(0);
        review_feedback = format!(
            "{}\n\nStrengths: {}",
            review["feedback"].as_str().unwrap_or(""),
            review["strengths"].as_str().unwrap_or("")
        );
        satisfied = parse_satisfied(&review);
        eprintln!("job {job_id}: reviewer {iters_run}/{max_iters} ✓ score={review_score} satisfied={satisfied} ({:.1}s)", t0.elapsed().as_secs_f64());

        // Early exit: score plateaued (improvement <5 for 2 consecutive iterations)
        if iteration > 0 {
            let improvement = review_score - prev_score;
            if improvement < 5 {
                plateau_count += 1;
                if plateau_count >= 2 {
                    eprintln!("job {job_id}: giving up — score plateaued (prev={prev_score}, now={review_score}, no improvement for {plateau_count} iterations)");
                    break;
                }
            } else {
                plateau_count = 0;
            }
        }
        prev_score = review_score;

        if iteration == 2 {
            eprintln!("job {job_id}: verifier (mid-loop) →");
            let t1 = std::time::Instant::now();
            let ver = verify_call(app, master_cv, &cv, vf_out, Some(&vf_eff)).await?;
            eprintln!("job {job_id}: verifier (mid-loop) ✓ truth={}% ({:.1}s)", ver["truth_pct"].as_i64().unwrap_or(0), t1.elapsed().as_secs_f64());
            if ver["truth_pct"].as_i64().unwrap_or(100) < 50 {
                fabrication_warning = true;
            }
        }

        if satisfied { break; }
        if iteration == max_iters - 1 { break; }

        let next_iter = iters_run + 1;
        db::set_progress(&app.db, job_id, &format!("Writer: revising draft ({next_iter}/{max_iters})…")).await?;
        publish_job_update(app, job_id, "generating", &format!("Writer: revising draft ({next_iter}/{max_iters})…"));
        let feedback_for_writer = if fabrication_warning {
            format!(
                "FABRICATION WARNING: The verifier found that more than 50% of claims \
                 are not supported by the master CV. Focus on using ONLY facts from the \
                 master CV. Do not fabricate skills or experience.\n\n{}",
                review_feedback
            )
        } else {
            review_feedback.clone()
        };
        let t1 = std::time::Instant::now();
        eprintln!("job {job_id}: writer (revision {iters_run}) →");
        check_cancelled(app)?;
        cv = writer_call(app, &wiki_context, jd, Some(&json!({
            "score": review_score,
            "feedback": feedback_for_writer,
            "strengths": review["strengths"],
        })), wr_out, Some(&wr_eff)).await?;
        eprintln!("job {job_id}: writer (revision {iters_run}) ✓ ({:.1}s)", t1.elapsed().as_secs_f64());
        writer_constraints = cv.get("constraints").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string());
        strip_cv_metadata(&mut cv);
        db::save_cv_draft(&app.db, job_id, cv.clone()).await?;
        db::set_progress(&app.db, job_id, &format!("Reviewer {}/{max_iters}: scoring draft…", iters_run + 1)).await?;
        publish_job_update(app, job_id, "generating", &format!("Reviewer {}/{max_iters}: scoring draft…", iters_run + 1));
    }

   // 3. Post-loop verifier
   check_cancelled(app)?;
   db::set_progress(&app.db, job_id, "Verifier: fact-checking CV…").await?;
   publish_job_update(app, job_id, "generating", "Verifier: fact-checking CV…");
   eprintln!("job {job_id}: verifier →");
    let t0 = std::time::Instant::now();
    let mut verification = verify_call(app, master_cv, &cv, vf_out, Some(&vf_eff)).await?;
    let final_truth_pct = verification["truth_pct"].as_i64().unwrap_or(100);
    eprintln!("job {job_id}: verifier ✓ truth={final_truth_pct}% ({:.1}s)", t0.elapsed().as_secs_f64());

   // 4. Editor fixup (if truth_pct < 50 — severe fabrication)
   check_cancelled(app)?;
   if final_truth_pct < 50 {
        db::set_progress(&app.db, job_id, &format!("Editor: fixing fabrication (truth={final_truth_pct}%)…")).await?;
        publish_job_update(app, job_id, "generating", &format!("Editor: fixing fabrication (truth={final_truth_pct}%)…"));
        eprintln!("job {job_id}: editor → (truth_pct={final_truth_pct})");
        let t1 = std::time::Instant::now();
        if let Ok(edited_cv) = editor_call(app, master_cv, &cv, ed_out, Some(&ed_eff)).await {
            eprintln!("job {job_id}: editor ✓ ({:.1}s)", t1.elapsed().as_secs_f64());
            let has_experiences = edited_cv["experiences"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            let has_bullets = edited_cv["experiences"]
                .as_array()
                .map(|a| a.iter().any(|e| e["bullet_points"].as_array().map(|b| !b.is_empty()).unwrap_or(false)))
                .unwrap_or(false);

            if has_experiences && has_bullets {
                cv = edited_cv;
                db::save_cv_draft(&app.db, job_id, cv.clone()).await?;
                verification = verify_call(app, master_cv, &cv, vf_out, Some(&vf_eff)).await?;
            } else {
                verification["fabrication_detected"] = json!(true);
            }
        }
    }

   // 5. Ranker
   check_cancelled(app)?;
   db::set_progress(&app.db, job_id, "Ranker: predicting HR approval…").await?;
    publish_job_update(app, job_id, "generating", "Ranker: predicting HR approval…");
    let rank = ranker_call(app, jd, &cv, rk_out, Some(&rk_eff)).await?;

    // 6. Save — prepend loop status so the user sees it in the UI
    let loop_status = if satisfied {
        format!("✓ Passed in {iters_run} iteration{}", if iters_run == 1 { "" } else { "s" })
    } else if plateau_count >= 2 {
        format!("⚠ Gave up at iteration {iters_run} — score plateaued at {review_score}/100")
    } else {
        format!("⚠ Loop exhausted ({iters_run}/{max_iters} iterations) — still needs work")
    };
    let review_feedback = format!("{loop_status}\n\n{review_feedback}");
    db::save_review(&app.db, job_id, review_score, &review_feedback).await?;
    db::save_verification(&app.db, job_id, &verification).await?;
    db::save_rank(&app.db, job_id, &rank).await?;
    db::set_status(&app.db, job_id, "generated").await?;
    publish_job_update(app, job_id, "generated", &format!("Score: {review_score}/100 — {loop_status}"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Scrape-then-pipeline: fetch job data from URL, then run the full pipeline.
pub async fn process_job(app: &AppState, job_id: Uuid) -> Result<()> {
    db::set_status(&app.db, job_id, "scraping").await?;
    publish_job_update(app, job_id, "scraping", "Scraping job listing…");

    let url      = db::get_job_url(&app.db, job_id).await?;
    let job_data = super::scraper::fetch_job(app, &url).await?;
    db::update_job_data(&app.db, job_id, &job_data).await?;
    eprintln!("job {job_id}: scraped \"{}\" ({})", job_data["title"].as_str().unwrap_or("?"), job_data["company"].as_str().unwrap_or("?"));

   let master_cv = db::get_master_cv(&app.db).await?;
   let jd = job_data["description"].as_str().unwrap_or("").to_string();

    match run_pipeline(app, job_id, &master_cv, &jd).await {
        Ok(()) => Ok(()),
        Err(e) if e.to_string() == "cancelled" => {
            db::set_status(&app.db, job_id, "cancelled").await.ok();
            publish_job_update(app, job_id, "cancelled", "Cancelled by user");
            Err(e)
        }
        Err(e) => Err(e),
    }
}

/// Manual-then-pipeline: skip scraping, read JD from DB, run the full pipeline.
pub async fn process_manual_job(app: &AppState, job_id: Uuid) -> Result<()> {
    db::set_status(&app.db, job_id, "pre_screening").await?;
    publish_job_update(app, job_id, "pre_screening", "Screening job description…");

    let rec = db::get_job(&app.db, job_id).await?;
    let master_cv = db::get_master_cv(&app.db).await?;
    eprintln!("job {job_id}: manual job \"{}\" ({})", rec.title, rec.company);

    match run_pipeline(app, job_id, &master_cv, &rec.description).await {
        Ok(()) => Ok(()),
        Err(e) if e.to_string() == "cancelled" => {
            db::set_status(&app.db, job_id, "cancelled").await.ok();
            publish_job_update(app, job_id, "cancelled", "Cancelled by user");
            Err(e)
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Simple regenerate (writer-only, no review loop)
// ---------------------------------------------------------------------------

pub async fn regenerate_cv(app: &AppState, job_id: Uuid, feedback: &str) -> Result<()> {
    db::set_status(&app.db, job_id, "generating").await?;
    db::set_progress(&app.db, job_id, "Writer: regenerating with feedback…").await?;
    publish_job_update(app, job_id, "generating", "Writer: regenerating with feedback…");

    let agent = db::get_agent_settings(&app.db).await.unwrap_or_default();
    let max_output = agent.max_output.max(256) as u32;
    let thinking_effort = agent.thinking_effort.clone();
    let ctx_window = agent.ctx_window.max(1000) as u32;
    let overrides = db::get_agent_overrides(&app.db).await.unwrap_or_default();
    let (wr_out, wr_eff) = {
        let o = overrides.get("writer");
        let mo = o.and_then(|x| x.max_output).map(|v| v.max(256) as u32).unwrap_or(max_output);
        let te = o.and_then(|x| x.thinking_effort.clone()).unwrap_or_else(|| thinking_effort.clone());
        (mo, te)
    };

    let master_cv = db::get_master_cv(&app.db).await?;
    let rec = db::get_job(&app.db, job_id).await?;
    let jd = rec.description;

    let wiki_context = build_wiki_context(app, &jd, &master_cv, ctx_window);
    let mut cv = writer_call(app, &wiki_context, &jd, Some(&json!({
        "score": 0,
        "feedback": feedback,
        "strengths": "",
    })), wr_out, Some(&wr_eff)).await?;
    strip_cv_metadata(&mut cv);
    db::save_cv_draft(&app.db, job_id, cv).await?;
    db::set_progress(&app.db, job_id, "").await?;
    db::set_status(&app.db, job_id, "generated").await?;
    publish_job_update(app, job_id, "generated", "Regeneration complete");
    Ok(())
}

// ---------------------------------------------------------------------------
// LLM role: Writer
