//! Pipeline module — LLM pipeline orchestration.
//!
//! Each LLM role (writer, reviewer, verifier, editor, ranker) lives in its own file.
//! The main orchestrator (process_job) and regeneration logic live in `process.rs`.

pub mod context;
pub mod editor;
pub mod helpers;
pub mod pre_screen;
pub mod process;
pub mod ranker;
pub mod reviewer;
pub mod scraper;
pub mod verifier;
pub mod writer;

pub use process::{process_job, process_manual_job, regenerate_cv};
