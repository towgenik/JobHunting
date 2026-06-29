//! LLM tool types — definitions for multi-turn tool-use.

use serde_json::Value;

/// A tool definition for the multi-turn loop.
pub struct ToolDef {
    pub name: &'static str,
    pub desc: &'static str,
    pub params: Value,
}

/// Result of a single tool call dispatch.
pub type ToolResult = std::result::Result<String, String>;
