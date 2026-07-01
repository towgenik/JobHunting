//! Pipeline re-export shim — backward compatibility for callers using `generate::`.
//!
//! All pipeline code now lives in `src/pipeline/`. This file re-exports the
//! public API so existing `generate::process_job` and `generate::regenerate_cv`
//! calls continue to work.

pub use crate::pipeline::process_job;
pub use crate::pipeline::process_manual_job;
pub use crate::pipeline::regenerate_cv;
