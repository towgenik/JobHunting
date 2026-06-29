//! Wiki module — graph engine, ingest, lint, and dispatch.
//!
//! The wiki is a knowledge graph of interlinked .md files.
//! WikiGraph loads and queries it. Ingest grows it from raw/ sources.
//! Lint self-heals by finding orphans and dangling links.

pub mod graph;
pub mod helpers;
pub mod ingest;
pub mod lint;

pub use graph::WikiGraph;
pub use ingest::{ingest, needs_ingest};
pub use lint::{lint, read_lint_report};
