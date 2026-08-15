//! LLM transport — request building, response parsing, retry harness.
//!
//! Multi-provider: OpenAI, OpenAI-compatible, Anthropic, Google Gemini.
//! Pipeline code uses call_llm_tool / call_llm_tool_loop only.

use std::collections::HashMap;
use anyhow::Result;
use serde_json::{json, Value};

use crate::AppState;
use super::Provider;
use super::types::{ToolDef, ToolResult};

/// Max retry attempts for transient LLM failures (empty response, 502, etc.).
pub const LLM_MAX_RETRIES: u32 = 3;

// ---------------------------------------------------------------------------
// Auth + request building
// ---------------------------------------------------------------------------

/// Build the reqwest::RequestBuilder with provider-specific auth headers.
pub fn build_llm_request(app: &AppState, body: &Value) -> reqwest::RequestBuilder {
   let cfg = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
    let url = resolve_endpoint(&cfg.endpoint, cfg.provider);
    let req = app.http.post(&url).json(body);
    match cfg.provider {
        Provider::Openai | Provider::OpenaiCompat => req.bearer_auth(&cfg.api_key),
        Provider::Anthropic => req
            .header("api-key", &cfg.api_key)
            .header("anthropic-version", "2023-06-01"),
        Provider::Google => req.header("x-goog-api-key", &cfg.api_key),
    }
}

/// Resolve a base endpoint URL to the full provider-specific path.
/// If the endpoint already ends with a known path, use it as-is.
/// Otherwise append the provider's canonical path:
///   OpenAI / OpenAI-compat: /chat/completions
///   Anthropic:              /messages
fn resolve_endpoint(endpoint: &str, provider: Provider) -> String {
    let base = endpoint.trim_end_matches('/');
    match provider {
        Provider::Openai | Provider::OpenaiCompat => {
            if base.ends_with("/chat/completions") {
                endpoint.to_string()
            } else {
                format!("{base}/chat/completions")
            }
        }
       Provider::Anthropic => {
           if base.ends_with("/messages") {
               endpoint.to_string()
           } else {
               if base.ends_with("/v1") {
                   format!("{base}/messages")
               } else {
                   format!("{base}/v1/messages")
               }
           }
       }
        Provider::Google => endpoint.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Provider-specific helpers
// ---------------------------------------------------------------------------

/// Map thinking-effort string to Anthropic `thinking` config (docs-verified).
fn anthropic_thinking_config(effort: Option<&str>) -> Value {
    match effort.unwrap_or("high") {
        "none" | "minimal" | "low" => json!({"type": "disabled"}),
        "medium" => json!({"type": "enabled", "budget_tokens": 8192}),
        "high" => json!({"type": "enabled", "budget_tokens": 16384}),
        "xhigh" => json!({"type": "enabled", "budget_tokens": 32768}),
        "adaptive" => json!({"type": "adaptive"}),
        _ => json!({"type": "enabled", "budget_tokens": 16384}),
    }
}

/// Map thinking-effort string to Gemini `thinkingBudget`.
fn gemini_thinking_budget(effort: Option<&str>) -> Option<i64> {
    match effort.unwrap_or("high") {
        "none" | "minimal" | "low" => Some(0),
        "medium" => Some(8192),
        "high" => Some(-1),
        "xhigh" => Some(-1),
        "adaptive" => Some(-1),
        _ => Some(-1),
    }
}

/// Convert messages to Gemini `contents` array, lifting leading system message
/// to `systemInstruction`.
fn gemini_contents(messages: &[Value]) -> (Vec<Value>, Value) {
    let mut system_instruction = Value::Null;
    let mut contents = Vec::new();
    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user");
        match role {
            "system" => {
                let text = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                system_instruction = json!({"parts": [{"text": text}]});
            }
            "assistant" | "model" => {
                // Handle already-Gemini-shaped parts (from tool loop)
                if let Some(parts_arr) = msg.get("parts") {
                    contents.push(json!({"role": "model", "parts": parts_arr.clone()}));
                } else {
                    let text = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    contents.push(json!({"role": "model", "parts": [{"text": text}]}));
                }
            }
            _ => {
                // user or tool-result turns (tool results already have parts)
                if let Some(parts_arr) = msg.get("parts") {
                    contents.push(json!({"role": "user", "parts": parts_arr.clone()}));
                } else {
                    let text = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    contents.push(json!({"role": "user", "parts": [{"text": text}]}));
                }
            }
        }
    }
    (contents, system_instruction)
}

/// Build a Gemini `:generateContent` request body with a single forced tool.
fn build_gemini_tool_request(
    messages: &[Value], max_tokens: u32, tool_name: &str, tool_desc: &str,
    schema: &Value, reasoning_effort: Option<&str>,
) -> Value {
    let (contents, sys) = gemini_contents(messages);
    let mut body = json!({
        "contents": contents,
        "tools": [{"functionDeclarations": [{
            "name": tool_name,
            "description": tool_desc,
            "parameters": schema,
        }]}],
        "toolConfig": {"functionCallingConfig": {"mode": "ANY"}},
        "generationConfig": {"maxOutputTokens": max_tokens},
    });
    if !sys.is_null() { body["systemInstruction"] = sys; }
    if let Some(budget) = gemini_thinking_budget(reasoning_effort) {
        body["generationConfig"]["thinkingConfig"] = json!({"thinkingBudget": budget});
    }
    body
}

/// Build a Gemini multi-tool request body with tool_choice=AUTO.
fn build_gemini_multi_tool_request(
    messages: &[Value], max_tokens: u32, tools: &[ToolDef],
    reasoning_effort: Option<&str>,
) -> Value {
    let (contents, sys) = gemini_contents(messages);
    let fns: Vec<Value> = tools.iter().map(|t| json!({
        "name": t.name, "description": t.desc, "parameters": t.params,
    })).collect();
    let mut body = json!({
        "contents": contents,
        "tools": [{"functionDeclarations": fns}],
        "toolConfig": {"functionCallingConfig": {"mode": "AUTO"}},
        "generationConfig": {"maxOutputTokens": max_tokens},
    });
    if !sys.is_null() { body["systemInstruction"] = sys; }
    if let Some(budget) = gemini_thinking_budget(reasoning_effort) {
        body["generationConfig"]["thinkingConfig"] = json!({"thinkingBudget": budget});
    }
    body
}

// ---------------------------------------------------------------------------
// Response extraction
// ---------------------------------------------------------------------------

/// Extract a JSON object from arbitrary text. Handles:
/// 1. Pure JSON
/// 2. JSON wrapped in markdown code fences
/// 3. JSON with trailing/leading prose
pub fn extract_json_from_text(content: &str) -> Option<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(content.trim()) {
        return Some(v);
    }
    let trimmed = content.trim();
    let unfenced = trimmed
        .strip_prefix("```json").or_else(|| trimmed.strip_prefix("```"))
        .map(|s| s.trim())
        .and_then(|s| s.strip_suffix("```").map(|s| s.trim()))
        .unwrap_or(trimmed);
    if let Ok(v) = serde_json::from_str::<Value>(unfenced) {
        return Some(v);
    }
    if let (Some(start), Some(end)) = (content.find('{'), content.rfind('}')) {
        if start < end {
            if let Ok(v) = serde_json::from_str::<Value>(&content[start..=end]) {
                return Some(v);
            }
        }
    }
    None
}

/// Validate that the output contains all required fields from the JSON schema.
pub fn validate_required_fields(output: &Value, schema: &Value) -> std::result::Result<(), String> {
    let Some(required) = schema.get("required").and_then(|r| r.as_array()) else {
        return Ok(());
    };
    for field in required {
        if let Some(name) = field.as_str() {
            if output.get(name).is_none() || output[name].is_null() {
                return Err(format!("missing required field: \x27{name}\x27"));
            }
        }
    }
    Ok(())
}

/// Extract structured output from an OpenAI-compatible LLM response.
pub fn extract_openai_output(resp: &Value) -> Result<Value> {
    if let Some(args_str) = resp
        .pointer("/choices/0/message/tool_calls/0/function/arguments")
        .and_then(|v| v.as_str())
    {
        return serde_json::from_str(args_str).map_err(|e| {
            anyhow::anyhow!("tool_call arguments not valid JSON: {e}\nraw: {args_str}")
        });
    }

    let content = resp
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "LLM response has no tool_calls and no message.content: {}",
                serde_json::to_string(&resp).unwrap_or_default().chars().take(500).collect::<String>()
            )
        })?;

    if let Some(v) = extract_json_from_text(content) {
        return Ok(v);
    }

    let finish = resp.pointer("/choices/0/finish_reason").and_then(|v| v.as_str()).unwrap_or("?");
    extract_xml_tool_call(content).ok_or_else(|| {
        anyhow::anyhow!(
            "could not extract JSON or XML tool call from content [finish_reason={finish}]:\n{content}"
        )
    })
}

/// Parse XML-style tool call format (fallback for reasoning models).
pub fn extract_xml_tool_call(content: &str) -> Option<Value> {
    let inner = content
        .find("\x3ctool_call\x3e")
        .and_then(|start| {
            let rest = &content[start + "\x3ctool_call\x3e".len()..];
            rest.find("\x3c/tool_call\x3e").map(|end| &rest[..end])
        })?;

    let mut map = serde_json::Map::new();
    let mut pos = 0;
    while let Some(tag_start) = inner[pos..].find("\x3cparameter=") {
        let abs_start = pos + tag_start + "\x3cparameter=".len();
        let after_key = &inner[abs_start..];
        let key_end = after_key.find('\x3e')?;
        let key = after_key[..key_end].to_string();

        let val_start = abs_start + key_end + 1;
        let val_end = inner[val_start..]
            .find("\x3c/parameter\x3e")
            .or_else(|| {
                inner[val_start..]
                    .find("\x3cparameter")
                    .or_else(|| inner[val_start..].find("\x3c/tool_call\x3e"))
            })
            .unwrap_or(inner.len() - val_start);

        let value = inner[val_start..val_start + val_end].trim().to_string();
        map.insert(key, json!(value));

        pos = val_start + val_end;
        if inner[val_start + val_end..].starts_with("\x3c/parameter\x3e") {
            pos += "\x3c/parameter\x3e".len();
        }
    }

    if map.is_empty() { return None; }
    Some(Value::Object(map))
}

/// Extract structured output from an Anthropic-native LLM response.
pub fn extract_anthropic_output(resp: &Value) -> Result<Value> {
    resp["content"]
        .as_array()
        .and_then(|blocks| blocks.iter().find(|b| b["type"] == "tool_use"))
        .and_then(|b| b.get("input"))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "LLM response missing tool_use block: {}",
                serde_json::to_string(&resp).unwrap_or_default().chars().take(500).collect::<String>()
            )
        })
        .cloned()
}

/// Extract structured output from a Google Gemini LLM response.
pub fn extract_gemini_output(resp: &Value) -> Result<Value> {
    let parts = resp
        .pointer("/candidates/0/content/parts")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!(
            "Gemini response missing candidates[0].content.parts: {}",
            serde_json::to_string(&resp).unwrap_or_default().chars().take(500).collect::<String>()
        ))?;

    // 1. Check for functionCall
    for part in parts {
        if let Some(fc) = part.get("functionCall") {
            if let Some(args) = fc.get("args") {
                return Ok(args.clone());
            }
        }
    }
    // 2. Check for text content
    let text: String = parts.iter()
        .filter_map(|p| p.get("text").and_then(|v| v.as_str()))
        .collect();
    if !text.is_empty() {
        if let Some(v) = extract_json_from_text(&text) {
            return Ok(v);
        }
    }
    anyhow::bail!("Gemini response has no functionCall or parseable text")
}

// ---------------------------------------------------------------------------
// Request body builders
// ---------------------------------------------------------------------------

/// Build the request body for an LLM call with a forced tool.
pub fn build_tool_request(
    model: &str,
    messages: &[Value],
    max_tokens: u32,
    tool_name: &str,
    tool_desc: &str,
    schema: &Value,
    provider: Provider,
    reasoning_effort: Option<&str>,
) -> Value {
   if provider == Provider::Google {
       return build_gemini_tool_request(messages, max_tokens, tool_name, tool_desc, schema, reasoning_effort);
   }

    if provider == Provider::Anthropic {
        return build_anthropic_tool_request(
            model, messages, max_tokens, tool_name, tool_desc, schema, reasoning_effort,
        );
    }

    // OpenAI-family: prefix caching is automatic for prompts >= 1024 tokens.
    // No prompt_cache_key — that field is Responses-API only, and adding it to
    // chat/completions may break OpenAI-compatible backends (e.g. vLLM).
    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": messages,
        "stream": false,
        "tools": [{
            "type": "function",
            "function": {
                "name": tool_name,
                "description": tool_desc,
                "parameters": schema,
            }
        }],
        "tool_choice": {"type": "function", "function": {"name": tool_name}},
    });
    if let Some(effort) = reasoning_effort {
        body["reasoning_effort"] = json!(effort);
    }

    body
}

/// Build an Anthropic Messages API request body with prompt caching.
///
/// Anthropic requires system prompts as a top-level `system` parameter, not
/// inside the `messages` array. Cache breakpoints (max 4):
///  1. Last system block  — stable across calls (master CV, wiki, rules)
///  2. Last tool definition — stable within a pipeline stage
///  3. Last user message   — caches the growing prefix in retry / multi-turn
fn build_anthropic_tool_request(
    model: &str,
    messages: &[Value],
    max_tokens: u32,
    tool_name: &str,
    tool_desc: &str,
    schema: &Value,
    reasoning_effort: Option<&str>,
) -> Value {
    let (mut system_blocks, convo) = split_system_messages(messages);
   let cache = || json!({"type": "ephemeral", "ttl": "5m"});

   let mut body = json!({
       "model": model,
       "max_tokens": max_tokens,
       "messages": convo,
       "tools": [{
           "name": tool_name,
           "description": tool_desc,
           "input_schema": schema,
           "cache_control": cache(),
       }],
       "tool_choice": {"type": "tool", "name": tool_name},
       "thinking": anthropic_thinking_config(reasoning_effort),
   });

   // Breakpoint 1: system (last block gets cache_control)
   if !system_blocks.is_empty() {
       if let Some(last) = system_blocks.last_mut() {
           last["cache_control"] = cache();
       }
       body["system"] = json!(system_blocks);
   }

    // Breakpoint 3: last user message (prefix caching for retries / multi-turn)
    if let Some(last_msg) = body["messages"].as_array_mut()
        .and_then(|m| m.last_mut())
    {
        last_msg["cache_control"] = cache();
    }

    body
}

/// Split messages into (system_blocks, conversation_messages).
/// Anthropic requires the system prompt as a top-level `system` parameter.
fn split_system_messages(messages: &[Value]) -> (Vec<Value>, Vec<Value>) {
    let mut system_blocks = Vec::new();
    let mut convo = Vec::new();
    for msg in messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        if role == "system" {
            let text = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
            system_blocks.push(json!({"type": "text", "text": text}));
        } else {
            convo.push(msg.clone());
        }
    }
    (system_blocks, convo)
}

/// Send a single LLM request and extract the structured output.
pub async fn send_llm_request(
    app: &AppState,
    body: &Value,
    provider: Provider,
) -> Result<Value> {
    let api_resp = build_llm_request(app, body).send().await?;
    let status = api_resp.status();
    let resp: Value = api_resp.json().await?;

    if !status.is_success() {
        let msg = resp
            .pointer("/error/message")
            .and_then(|m| m.as_str())
            .or_else(|| resp.get("error").and_then(|e| e.as_str()))
            .unwrap_or("unknown error");
       anyhow::bail!("LLM API error {status}: {msg}");
   }

    log_cache_usage(&resp, provider);

    match provider {
        Provider::Openai | Provider::OpenaiCompat => extract_openai_output(&resp),
        Provider::Anthropic => extract_anthropic_output(&resp),
        Provider::Google => extract_gemini_output(&resp),
    }
}

/// Log prompt-caching token usage from the API response.
fn log_cache_usage(resp: &Value, provider: Provider) {
    match provider {
        Provider::Anthropic => {
            let read = resp.pointer("/usage/cache_read_input_tokens")
                .and_then(|v| v.as_u64()).unwrap_or(0);
            let created = resp.pointer("/usage/cache_creation_input_tokens")
                .and_then(|v| v.as_u64()).unwrap_or(0);
            if read > 0 || created > 0 {
                eprintln!("cache: read={read} created={created} tokens");
            }
        }
        Provider::Openai | Provider::OpenaiCompat => {
            let read = resp.pointer("/usage/prompt_tokens_details/cached_tokens")
                .and_then(|v| v.as_u64()).unwrap_or(0);
            if read > 0 {
                eprintln!("cache: read={read} cached tokens");
            }
        }
        Provider::Google => {}
    }
}

// ---------------------------------------------------------------------------
// Core LLM harness
// ---------------------------------------------------------------------------

/// Test LLM connection with a minimal prompt. Returns Ok(latency_ms) or Err.
pub async fn test_llm_connection(app: &AppState) -> Result<u64> {
    let (model, provider) = {
        let cfg = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
        (cfg.model.clone(), cfg.provider)
    };
    let body = match provider {
        Provider::Openai | Provider::OpenaiCompat => json!({
            "model": model,
            "messages": [{"role": "user", "content": "Say hi"}],
            "max_tokens": 16,
            "stream": false,
        }),
        // Use 256 tokens — reasoning models burn most of the budget on
        // `thinking` blocks before producing any `text` block.
        Provider::Anthropic => json!({
            "model": model,
            "max_tokens": 256,
            "messages": [{"role": "user", "content": "Say hi. Reply with one word."}],
        }),
        Provider::Google => json!({
            "contents": [{"role": "user", "parts": [{"text": "Say hi"}]}],
            "generationConfig": {"maxOutputTokens": 16},
        }),
    };
    let start = std::time::Instant::now();
    let req = build_llm_request(app, &body);
    let resp = req.send().await?;
    let status = resp.status();
    let elapsed = start.elapsed().as_millis() as u64;
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {status}: {text}");
    }
    let json: Value = resp.json().await?;
    let has_content = match provider {
        Provider::Openai | Provider::OpenaiCompat =>
            json.pointer("/choices/0/message/content").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false)
            || json.pointer("/choices/0/message/reasoning_content").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false),
        // Accept any non-empty content array (thinking, text, or tool_use blocks).
        // A `thinking`-only response still proves the endpoint is reachable and
        // responding in Anthropic Messages API format.
        Provider::Anthropic =>
            json.get("content").and_then(|c| c.as_array()).map(|a| !a.is_empty()).unwrap_or(false),
        Provider::Google =>
            json.pointer("/candidates/0/content/parts/0/text").and_then(|v| v.as_str()).is_some()
            || json.pointer("/candidates/0/content/parts/0/functionCall").is_some(),
    };
    if has_content {
        Ok(elapsed)
    } else {
        anyhow::bail!("No content in response: {}", serde_json::to_string(&json).unwrap_or_default().chars().take(200).collect::<String>())
    }
}

/// The core LLM harness. Guarantees structured output through:
/// 1. Native tool use (forced tool_choice)
/// 2. Retry with backoff for transient errors
/// 3. Self-correction for schema validation failures
pub async fn call_llm_tool(
    app: &AppState,
    prompt: &str,
    max_tokens: u32,
    tool_name: &str,
    tool_desc: &str,
    schema: &Value,
    mock: Value,
    reasoning_effort: Option<&str>,
) -> Result<Value> {
    call_llm_tool_with_progress(app, prompt, max_tokens, tool_name, tool_desc, schema, mock, reasoning_effort, None, None).await
}

/// Same as call_llm_tool but with optional progress callback after semaphore acquired.
pub async fn call_llm_tool_with_progress(
    app: &AppState,
    prompt: &str,
    max_tokens: u32,
    tool_name: &str,
    tool_desc: &str,
    schema: &Value,
    mock: Value,
    reasoning_effort: Option<&str>,
    job_id: Option<uuid::Uuid>,
    progress_msg: Option<&str>,
) -> Result<Value> {
    let (mock_llm, model, provider) = {
        let cfg = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
        (cfg.mock_llm, cfg.model.clone(), cfg.provider)
    };
    if mock_llm {
        return Ok(mock);
    }

    let sem = app.llm_semaphore.read().unwrap_or_else(|e| e.into_inner()).clone();
    let _permit = sem.acquire().await
        .map_err(|e| anyhow::anyhow!("semaphore error: {e}"))?;

    // Set progress AFTER semaphore acquired — job is now actively being processed
    if let (Some(jid), Some(msg)) = (job_id, progress_msg) {
        let _ = crate::db::set_progress(&app.db, jid, msg).await;
        crate::events::publish_job_update(app, jid, "pre_screening", msg);
    }

    let mut messages = vec![json!({"role": "user", "content": prompt})];
    let mut last_error: Option<String> = None;

    for attempt in 0..LLM_MAX_RETRIES {
        if let Some(err) = &last_error {
            messages.push(json!({
                "role": "user",
                "content": format!(
                    "Your previous response was rejected: {err}\n\
                     Please call {tool_name} again with a valid response that \
                     includes ALL required fields."
                )
            }));
        }

        let body = build_tool_request(
            &model, &messages, max_tokens, tool_name, tool_desc, schema, provider, reasoning_effort,
        );

        match send_llm_request(app, &body, provider).await {
            Ok(output) => {
                match validate_required_fields(&output, schema) {
                    Ok(()) => return Ok(output),
                    Err(e) => {
                        eprintln!(
                            "call_llm_tool[{tool_name}]: schema validation failed \
                             (attempt {}/{LLM_MAX_RETRIES}): {e}",
                            attempt + 1,
                        );
                        last_error = Some(e);
                        tokio::time::sleep(std::time::Duration::from_secs(
                            2u64.pow(attempt),
                        ))
                        .await;
                    }
                }
            }
            Err(e) => {
                let msg = format!("{e}");
                eprintln!(
                    "call_llm_tool[{tool_name}]: request failed \
                     (attempt {}/{LLM_MAX_RETRIES}): {msg}",
                    attempt + 1,
                );

                if msg.contains("tool") && (msg.contains("400") || msg.contains("Bad Request")) {
                    eprintln!(
                        "call_llm_tool[{tool_name}]: proxy rejected tool_choice, \
                         falling back to JSON mode"
                    );
                    return call_llm_json_fallback(
                        app, prompt, max_tokens, tool_name, tool_desc, schema, mock.clone(),
                    )
                    .await;
                }

                last_error = Some(msg.clone());

                let is_transient = msg.contains("502")
                    || msg.contains("503")
                    || msg.contains("504")
                    || msg.contains("429")
                    || msg.contains("500")
                    || msg.contains("timed out")
                    || msg.contains("connection")
                    || msg.contains("empty response");

                if !is_transient || attempt + 1 >= LLM_MAX_RETRIES {
                    return Err(e);
                }

                tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
            }
        }
    }

    anyhow::bail!(
        "call_llm_tool[{tool_name}]: exhausted {LLM_MAX_RETRIES} attempts. Last error: {}",
        last_error.unwrap_or_else(|| "unknown".into())
    )
}

/// Fallback: JSON mode when the proxy doesn't support tool_choice.
pub async fn call_llm_json_fallback(
    app: &AppState,
    prompt: &str,
    max_tokens: u32,
    tool_name: &str,
    tool_desc: &str,
    schema: &Value,
    _mock: Value,
) -> Result<Value> {
    let (model, provider) = {
        let cfg = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
        (cfg.model.clone(), cfg.provider)
    };

    let schema_str = serde_json::to_string_pretty(schema).unwrap_or_default();
    let prompt_with_schema = format!(
        "{prompt}\n\n### OUTPUT FORMAT\n{tool_desc}. \
         Respond with ONLY a valid JSON object (no markdown fences, no prose) \
         matching this schema:\n{schema_str}"
    );

    // Gemini has its own JSON fallback path
    if provider == Provider::Google {
        let body = json!({
            "contents": [{"role": "user", "parts": [{"text": prompt_with_schema}]}],
            "generationConfig": {
                "maxOutputTokens": max_tokens,
                "responseMimeType": "application/json",
            },
        });
        let mut last_error: Option<String> = None;
        for attempt in 0..LLM_MAX_RETRIES {
            let api_resp = build_llm_request(app, &body).send().await?;
            let status = api_resp.status();
            let resp: Value = api_resp.json().await?;
            if !status.is_success() {
                let msg = resp.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("unknown");
                anyhow::bail!("Gemini API error {status}: {msg}");
            }
            let parts = resp.pointer("/candidates/0/content/parts").and_then(|v| v.as_array());
            let text: String = parts.map(|p| p.iter().filter_map(|v| v.get("text").and_then(|t| t.as_str())).collect()).unwrap_or_default();
            match extract_json_from_text(&text) {
                Some(output) => match validate_required_fields(&output, schema) {
                    Ok(()) => return Ok(output),
                    Err(e) => {
                        eprintln!("call_llm_json_fallback[gemini, {tool_name}]: validation failed (attempt {}): {e}", attempt + 1);
                        last_error = Some(e);
                    }
                },
                None => { last_error = Some(format!("could not parse JSON from Gemini response")); }
            }
            if attempt + 1 < LLM_MAX_RETRIES {
                tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
            }
        }
        anyhow::bail!("call_llm_json_fallback[gemini, {tool_name}]: exhausted retries. Last error: {}", last_error.unwrap_or_else(|| "unknown".into()));
    }

    let mut messages = vec![json!({"role": "user", "content": prompt_with_schema})];
    let mut last_error: Option<String> = None;

    for attempt in 0..LLM_MAX_RETRIES {
        if let Some(err) = &last_error {
            messages.push(json!({
                "role": "user",
                "content": format!("Your previous response was invalid: {err}. Output ONLY valid JSON.")
            }));
        }

        let body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "stream": false,
            "response_format": {"type": "json_object"},
            "messages": messages,
        });

        let api_resp = build_llm_request(app, &body).send().await?;
        let status = api_resp.status();
        let resp: Value = api_resp.json().await?;

        if !status.is_success() {
            let msg = resp.pointer("/error/message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown");
            anyhow::bail!("LLM API error {status}: {msg}");
        }

        let content = resp.pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("response missing content"))?;

        match extract_json_from_text(content) {
            Some(output) => match validate_required_fields(&output, schema) {
                Ok(()) => return Ok(output),
                Err(e) => {
                    eprintln!("call_llm_json_fallback[{tool_name}]: validation failed (attempt {}): {e}", attempt + 1);
                    last_error = Some(e);
                }
            },
            None => {
                let finish = resp.pointer("/choices/0/finish_reason").and_then(|v| v.as_str()).unwrap_or("?");
                last_error = Some(format!("could not parse JSON [finish={finish}]"));
            }
        }

        if attempt + 1 < LLM_MAX_RETRIES {
            tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
        }
    }

    anyhow::bail!(
        "call_llm_json_fallback[{tool_name}]: exhausted retries. Last error: {}",
        last_error.unwrap_or_else(|| "unknown".into())
    )
}

// ---------------------------------------------------------------------------
// Multi-turn tool-use loop (for ingest / agent operations)
// ---------------------------------------------------------------------------

/// Run a multi-turn tool-use loop. The LLM may call any of the registered
/// tools repeatedly until it emits a final assistant text message.
pub async fn call_llm_tool_loop(
    app: &AppState,
    system: &str,
    user_msg: &str,
    tools: &[ToolDef],
    dispatch: impl Fn(&str, &Value) -> ToolResult,
    max_tokens: u32,
    reasoning_effort: Option<&str>,
    max_iterations: u32,
) -> Result<String> {
    let (mock_llm, model, provider) = {
        let cfg = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
        (cfg.mock_llm, cfg.model.clone(), cfg.provider)
    };
    if mock_llm {
        return Ok("Mock: ingest complete.".into());
    }

    let sem = app.llm_semaphore.read().unwrap_or_else(|e| e.into_inner()).clone();
    let _permit = sem.acquire().await
        .map_err(|e| anyhow::anyhow!("semaphore error: {e}"))?;

    let mut messages = vec![
        json!({"role": "system", "content": system}),
        json!({"role": "user", "content": user_msg}),
    ];

    for _iteration in 0..max_iterations {
        let body = build_multi_tool_request(
            &model, &messages, max_tokens, tools, provider, reasoning_effort,
        );
        let api_resp = build_llm_request(app, &body).send().await?;
        let status = api_resp.status();
        let resp: Value = api_resp.json().await?;

        if !status.is_success() {
            let msg = resp.pointer("/error/message").and_then(|m| m.as_str())
                .or_else(|| resp.get("error").and_then(|e| e.as_str()))
                .unwrap_or("unknown error");
           anyhow::bail!("LLM API error {status}: {msg}");
       }

        log_cache_usage(&resp, provider);

        if provider.is_openai_family() {
            let tool_calls = resp.pointer("/choices/0/message/tool_calls")
                .and_then(|v| v.as_array());

            if let Some(calls) = tool_calls {
                if !calls.is_empty() {
                    let assistant_msg = resp.pointer("/choices/0/message").cloned()
                        .unwrap_or(json!({}));
                    messages.push(assistant_msg);

                    for call in calls {
                        let id = call["id"].as_str().unwrap_or("");
                        let name = call["function"]["name"].as_str().unwrap_or("");
                        let args_raw = &call["function"]["arguments"];
                        let args: Value = if let Some(s) = args_raw.as_str() {
                            serde_json::from_str(s).unwrap_or(Value::Null)
                        } else if args_raw.is_object() {
                            args_raw.clone()
                        } else {
                            Value::Null
                        };

                        let result = dispatch(name, &args);
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": id,
                            "content": match result {
                                Ok(s) => s,
                                Err(e) => format!("Error: {e}"),
                            }
                        }));
                    }
                    continue;
                }
            }

            let content = resp.pointer("/choices/0/message/content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            return Ok(content.to_string());
        } else if provider == Provider::Anthropic {
            let content_blocks = resp["content"].as_array();
            let tool_uses: Vec<&Value> = content_blocks
                .map(|blocks| blocks.iter().filter(|b| b["type"] == "tool_use").collect())
                .unwrap_or_default();

            if !tool_uses.is_empty() {
                messages.push(json!({"role": "assistant", "content": content_blocks.unwrap_or(&vec![])}));

                let mut results = Vec::new();
                for tu in &tool_uses {
                    let id = tu["id"].as_str().unwrap_or("");
                    let name = tu["name"].as_str().unwrap_or("");
                    let input = tu["input"].clone();

                    let result = dispatch(name, &input);
                   results.push(json!({
                       "type": "tool_result",
                       "tool_use_id": id,
                       "content": match result {
                           Ok(s) => s,
                           Err(e) => format!("Error: {e}"),
                       }
                   }));
               }
                // Cache_control on last tool_result caches the growing prefix.
                if let Some(last) = results.last_mut() {
                    last["cache_control"] = json!({"type": "ephemeral", "ttl": "5m"});
                }
                messages.push(json!({"role": "user", "content": results}));
                continue;
            }

            let text: String = content_blocks
                .map(|blocks| blocks.iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join(""))
                .unwrap_or_default();
            return Ok(text);
        } else {
            // Google Gemini multi-turn loop
            let parts = resp.pointer("/candidates/0/content/parts").and_then(|v| v.as_array());
            let function_calls: Vec<&Value> = parts
                .map(|p| p.iter().filter(|part| part.get("functionCall").is_some()).collect())
                .unwrap_or_default();

            if !function_calls.is_empty() {
                // Push model turn with functionCall parts
                let model_parts: Vec<Value> = function_calls.iter().map(|p| (*p).clone()).collect();
                messages.push(json!({"role": "model", "parts": model_parts}));

                // Build tool result parts for the user turn
                let mut result_parts = Vec::new();
                for fc_part in &function_calls {
                    let fc = &fc_part["functionCall"];
                    let name = fc["name"].as_str().unwrap_or("");
                    let args = fc["args"].clone();
                    let result = dispatch(name, &args);
                    result_parts.push(json!({
                        "functionResponse": {
                            "name": name,
                            "response": {"result": match result {
                                Ok(s) => s,
                                Err(e) => format!("Error: {e}"),
                            }}
                        }
                    }));
                }
                messages.push(json!({"role": "user", "parts": result_parts}));
                continue;
            }

            // No function calls — extract text and return
            let text: String = parts
                .map(|p| p.iter()
                    .filter_map(|part| part.get("text").and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join(""))
                .unwrap_or_default();
            return Ok(text);
        }
    }

    anyhow::bail!("tool loop exhausted {max_iterations} iterations")
}

/// Build a request body with multiple tools and tool_choice=auto.
pub fn build_multi_tool_request(
    model: &str,
    messages: &[Value],
    max_tokens: u32,
    tools: &[ToolDef],
    provider: Provider,
    reasoning_effort: Option<&str>,
) -> Value {
   if provider == Provider::Google {
       return build_gemini_multi_tool_request(messages, max_tokens, tools, reasoning_effort);
   }

    if provider == Provider::Anthropic {
        return build_anthropic_multi_tool_request(
            model, messages, max_tokens, tools, reasoning_effort,
        );
    }

    // OpenAI-family: automatic prefix caching for prompts >= 1024 tokens
    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": messages,
        "stream": false,
        "tools": tools.iter().map(|t| json!({
            "type": "function",
            "function": {
                "name": t.name,
                "description": t.desc,
                "parameters": t.params,
            }
        })).collect::<Vec<_>>(),
        "tool_choice": "auto",
    });
    if let Some(effort) = reasoning_effort {
        body["reasoning_effort"] = json!(effort);
    }

    body
}

/// Build an Anthropic multi-tool request with prompt caching.
/// Same breakpoint strategy as the single-tool variant:
///  1. Last system block, 2. Last tool definition, 3. Last user message
fn build_anthropic_multi_tool_request(
    model: &str,
    messages: &[Value],
    max_tokens: u32,
    tools: &[ToolDef],
    reasoning_effort: Option<&str>,
) -> Value {
    let (mut system_blocks, convo) = split_system_messages(messages);
    let cache = || json!({"type": "ephemeral", "ttl": "5m"});

    let mut tool_defs: Vec<Value> = tools.iter().map(|t| json!({
        "name": t.name,
        "description": t.desc,
        "input_schema": t.params,
    })).collect();
    // Breakpoint 2: cache_control on the last tool
    if let Some(last_tool) = tool_defs.last_mut() {
        last_tool["cache_control"] = cache();
    }

    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": convo,
        "tools": tool_defs,
        "tool_choice": {"type": "auto"},
        "thinking": anthropic_thinking_config(reasoning_effort),
    });

    // Breakpoint 1: system
    if !system_blocks.is_empty() {
        if let Some(last) = system_blocks.last_mut() {
            last["cache_control"] = cache();
        }
        body["system"] = json!(system_blocks);
    }

    // Breakpoint 3: last user message (caches growing conversation prefix)
    if let Some(last_msg) = body["messages"].as_array_mut()
        .and_then(|m| m.last_mut())
    {
        last_msg["cache_control"] = cache();
    }

    body
}

// ---------------------------------------------------------------------------
// Model discovery + capability lookup
// ---------------------------------------------------------------------------

/// Resolve a base endpoint URL to the provider's model-listing path.
fn resolve_models_endpoint(endpoint: &str, provider: Provider) -> String {
    let base = endpoint.trim_end_matches('/');
    match provider {
        Provider::Openai | Provider::OpenaiCompat => {
            // Strip any known inference suffix, then append /models
            let b = base
                .strip_suffix("/chat/completions")
                .unwrap_or(base);
            format!("{b}/models")
        }
        Provider::Anthropic => {
            if base.ends_with("/v1") {
                format!("{base}/models")
            } else {
                format!("{base}/v1/models")
            }
        }
        Provider::Google => {
            // Gemini models are listed under /v1beta/models
            if base.contains("/v1beta") {
                format!("{base}/models")
            } else {
                format!("{base}/v1beta/models")
            }
        }
    }
}

/// Extract the origin (scheme + host) from a URL string.
fn extract_origin(url: &str) -> String {
    if let Some(start) = url.find("://") {
        let after_protocol = &url[start + 3..];
        if let Some(end) = after_protocol.find('/') {
            format!("{}://{}", &url[..start], &after_protocol[..end])
        } else {
            url.to_string()
        }
    } else {
        url.to_string()
    }
}

/// Try fetching models from an OpenAI-compatible /v1/models endpoint.
async fn fetch_models_openai_compat(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<ModelInfo>> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let resp = http.get(&url).bearer_auth(api_key).send().await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    if !status.is_success() {
        let msg = body.pointer("/error/message")
            .and_then(|v| v.as_str())
            .or_else(|| body.get("error").and_then(|e| e.as_str()))
            .unwrap_or("unknown error");
        anyhow::bail!("HTTP {status}: {msg}");
    }
    let mut models = Vec::new();
    if let Some(arr) = body.get("data").and_then(|d| d.as_array()) {
        for m in arr {
            let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if id.is_empty() { continue; }
            let ctx = m.get("context_length")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            let max_out = m.get("top_provider")
                .and_then(|tp| tp.get("max_completion_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            models.push(ModelInfo { id, context_window: ctx, max_output: max_out });
        }
    }
    if models.is_empty() {
        anyhow::bail!("no models returned by endpoint");
    }
    Ok(models)
}

/// Fetch the list of available model IDs from the configured provider.
pub async fn fetch_models(app: &AppState) -> Result<Vec<ModelInfo>> {
    let (endpoint, api_key, provider) = {
        let cfg = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
        (cfg.endpoint.clone(), cfg.api_key.clone(), cfg.provider)
    };

    // Anthropic: try native endpoint first, fall back to OpenAI /v1/models
    // because lightweight proxies only implement the OpenAPI-compatible route.
    if provider == Provider::Anthropic {
        let anthropic_url = resolve_models_endpoint(&endpoint, Provider::Anthropic);
        match app.http.get(&anthropic_url)
            .header("api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .send().await
        {
            Ok(resp) if resp.status().is_success() => {
                let body: Value = resp.json().await?;
                let mut models = Vec::new();
                if let Some(arr) = body.get("data").and_then(|d| d.as_array()) {
                    for m in arr {
                        let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        if id.is_empty() { continue; }
                        let ctx = m.get("context_length")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u32);
                        let max_out = m.get("top_provider")
                            .and_then(|tp| tp.get("max_completion_tokens"))
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u32);
                        models.push(ModelInfo { id, context_window: ctx, max_output: max_out });
                    }
                }
                if models.is_empty() {
                    anyhow::bail!("no models returned by Anthropic endpoint");
                }
                return Ok(models);
            }
            _ => {
                // Fallback to OpenAI-compatible /v1/models
                let origin = extract_origin(&endpoint);
                return fetch_models_openai_compat(&app.http, &origin, &api_key).await;
            }
        }
    }

    let url = resolve_models_endpoint(&endpoint, provider);
    let req = match provider {
        Provider::Openai | Provider::OpenaiCompat => app.http.get(&url).bearer_auth(&api_key),
        Provider::Google => app.http.get(&url).header("x-goog-api-key", &api_key),
        _ => unreachable!(),
    };
    let resp = req.send().await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    if !status.is_success() {
        let msg = body.pointer("/error/message")
            .and_then(|v| v.as_str())
            .or_else(|| body.get("error").and_then(|e| e.as_str()))
            .unwrap_or("unknown error");
        anyhow::bail!("HTTP {status}: {msg}");
    }
    let mut models = Vec::new();
    match provider {
        Provider::Openai | Provider::OpenaiCompat => {
            if let Some(arr) = body.get("data").and_then(|d| d.as_array()) {
                for m in arr {
                    let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    if id.is_empty() { continue; }
                    let ctx = m.get("context_length")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32);
                    let max_out = m.get("top_provider")
                        .and_then(|tp| tp.get("max_completion_tokens"))
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32);
                    models.push(ModelInfo { id, context_window: ctx, max_output: max_out });
                }
            }
        }
        Provider::Google => {
            if let Some(arr) = body.get("models").and_then(|m| m.as_array()) {
                for m in arr {
                    let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let id = name.strip_prefix("models/").unwrap_or(name).to_string();
                    if id.is_empty() { continue; }
                    let ctx = m.get("inputTokenLimit").and_then(|v| v.as_u64()).map(|v| v as u32);
                    models.push(ModelInfo { id, context_window: ctx, max_output: None });
                }
            }
        }
        // Handled before the match — unreachable here
        Provider::Anthropic => unreachable!(),
    }
    if models.is_empty() {
        anyhow::bail!("no models returned by endpoint");
    }
    Ok(models)
}

/// Discovered model with optional context window (only some providers return it).
pub struct ModelInfo {
    pub id:             String,
    pub context_window: Option<u32>,
    pub max_output:     Option<u32>,
}

/// Result of a capability lookup for a single model.
pub struct ModelCapabilities {
    pub ctx_window: u32,
    pub max_output: u32,
    pub source:     &'static str, // "api" | "api-partial"
}

/// Fetch model capabilities from the Xiaomi public metadata API.
/// Returns a map of model ID → (context_length, max_output_length).
async fn fetch_xiaomi_model_caps(http: &reqwest::Client) -> HashMap<String, (u32, u32)> {
    let url = "https://platform.xiaomimimo.com/api/v1/models";
    let Ok(resp) = http.get(url).send().await else { return HashMap::new() };
    let Ok(body) = resp.json::<Value>().await else { return HashMap::new() };
    let mut map = HashMap::new();
    if let Some(arr) = body.get("data").and_then(|d| d.as_array()) {
        for m in arr {
            let id = match m.get("id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };
            let Some(ctx) = m.get("context_length").and_then(|v| v.as_u64()).map(|v| v as u32) else { continue };
            let Some(max_out) = m.get("max_output_length").and_then(|v| v.as_u64()).map(|v| v as u32) else { continue };
            map.insert(id, (ctx, max_out));
        }
    }
    map
}

/// Look up ctx_window + max_output for a model from the endpoint's own APIs.
/// Returns error if endpoint provides no capability data — no hardcoded fallbacks.
pub async fn fetch_capabilities(app: &AppState, model: &str) -> Result<ModelCapabilities> {
    // 1. Try the provider-specific models endpoint.
    if let Ok(models) = fetch_models(app).await {
        if let Some(found) = models.iter().find(|m| m.id == model || model.contains(&m.id)) {
            if let Some(ctx) = found.context_window {
                let max_out = found.max_output.unwrap_or(0);
                let source = if max_out > 0 { "api" } else { "api-partial" };
                return Ok(ModelCapabilities {
                    ctx_window: ctx,
                    max_output: max_out,
                    source,
                });
            }
        }
    }

    // 2. Try Xiaomi public metadata API (unauthenticated — works for all Xiaomi endpoints).
    let xiaomi_caps = fetch_xiaomi_model_caps(&app.http).await;
    for (meta_id, &(ctx, max_out)) in &xiaomi_caps {
        let model_lower = model.to_ascii_lowercase();
        let meta_lower = meta_id.to_ascii_lowercase();
        if model_lower.contains(&meta_lower) || meta_lower.contains(&model_lower) {
            return Ok(ModelCapabilities {
                ctx_window: ctx,
                max_output: max_out,
                source: "api",
            });
        }
    }

    anyhow::bail!("endpoint does not expose model context limits")
}
