//! LLM transport layer — request building, response parsing, retry harness.
//!
//! Pipeline code calls `call_llm_tool` / `call_llm_tool_loop` only.
//! Transport internals (build_*, extract_*, validate_*) are in `transport.rs`.

pub mod transport;
pub mod types;
pub mod provider;

pub use transport::{
    call_llm_tool,
    call_llm_tool_with_progress,
    call_llm_tool_loop,
};
pub use types::ToolDef;
pub use provider::Provider;
pub use transport::{fetch_models, fetch_capabilities};
