//! Provider enum — dispatches auth headers, request shape, and response parsing
//! across OpenAI, OpenAI-compatible, Anthropic, and Google Gemini APIs.

use std::fmt;

/// Supported LLM upstream providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Openai,
    OpenaiCompat,
    Anthropic,
    Google,
}

impl Provider {
    /// Parse from a settings string (case-insensitive).
    /// Accepts: "openai", "openai-compat", "anthropic", "google".
    /// Anything else falls back to OpenaiCompat.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "openai"       => Provider::Openai,
            "anthropic"    => Provider::Anthropic,
            "google"       => Provider::Google,
            "openai-compat" | "openai_compat" | "compatible" => Provider::OpenaiCompat,
            _ => Provider::OpenaiCompat,
        }
    }

    /// Auto-detect provider from an endpoint URL.
    pub fn from_endpoint(url: &str) -> Self {
        let lower = url.to_ascii_lowercase();
        if lower.contains("anthropic.com") {
            Provider::Anthropic
        } else if lower.contains("generativelanguage.googleapis.com")
            || lower.contains("aiplatform.googleapis.com")
            || lower.contains("vertexai")
        {
            Provider::Google
        } else if lower.contains("api.openai.com") {
            Provider::Openai
        } else {
            Provider::OpenaiCompat
        }
    }

    /// True for Openai and OpenaiCompat (same request/response shape).
    pub fn is_openai_family(&self) -> bool {
        matches!(self, Provider::Openai | Provider::OpenaiCompat)
    }

    /// Persistable string key used in the DB column.
    pub fn as_str(&self) -> &'static str {
        match self {
            Provider::Openai       => "openai",
            Provider::OpenaiCompat => "openai-compat",
            Provider::Anthropic    => "anthropic",
            Provider::Google       => "google",
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Default for Provider {
    fn default() -> Self {
        Provider::OpenaiCompat
    }
}
