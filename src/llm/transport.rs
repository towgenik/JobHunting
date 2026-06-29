//! LLM transport — request building, response parsing, retry harness.
//!
//! ponytail: all HTTP/JSON plumbing for LLM calls lives here.
//! Pipeline code uses call_llm_tool / call_llm_tool_loop only.

use anyhow::Result;
use serde_json::{json, Value};

use crate::AppState;

/// Max retry attempts for transient LLM failures (empty response, 502, etc.).
pub const LLM_MAX_RETRIES: u32 = 3;

/// Build the reqwest::RequestBuilder with auth headers (OpenAI Bearer vs Anthropic x-api-key).
pub fn build_llm_request(app: &AppState, body: &Value) -> reqwest::RequestBuilder {
    let cfg = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
    let req = app.http.post(&cfg.endpoint).json(body);
    if cfg.openai_compat {
        req.bearer_auth(&cfg.api_key)
    } else {
        req.header("x-api-key", &cfg.api_key)
           .header("anthropic-version", "2023-06-01")
    }
}

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

/// Build the request body for an LLM call with a forced tool.
pub fn build_tool_request(
    model: &str,
    messages: &[Value],
    max_tokens: u32,
    tool_name: &str,
    tool_desc: &str,
    schema: &Value,
    openai_compat: bool,
    reasoning_effort: Option<&str>,
) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": messages,
    });

    if openai_compat {
        body["stream"] = json!(false);
        body["tools"] = json!([{
            "type": "function",
            "function": {
                "name": tool_name,
                "description": tool_desc,
                "parameters": schema,
            }
        }]);
        body["tool_choice"] = json!({"type": "function", "function": {"name": tool_name}});
        if let Some(effort) = reasoning_effort {
            body["reasoning_effort"] = json!(effort);
        }
    } else {
        body["tools"] = json!([{
            "name": tool_name,
            "description": tool_desc,
            "input_schema": schema,
        }]);
        body["tool_choice"] = json!({"type": "tool", "name": tool_name});
        let effort = reasoning_effort.unwrap_or("high");
        body["reasoning"] = json!({"effort": effort});
    }

    body
}

/// Send a single LLM request and extract the structured output.
pub async fn send_llm_request(
    app: &AppState,
    body: &Value,
    openai_compat: bool,
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

    if openai_compat {
        extract_openai_output(&resp)
    } else {
        extract_anthropic_output(&resp)
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
    let (mock_llm, model, openai_compat) = {
        let cfg = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
        (cfg.mock_llm, cfg.model.clone(), cfg.openai_compat)
    };
    if mock_llm {
        return Ok(mock);
    }

    let _permit = app
        .llm_semaphore
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("semaphore error: {e}"))?;

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
            &model, &messages, max_tokens, tool_name, tool_desc, schema, openai_compat, reasoning_effort,
        );

        match send_llm_request(app, &body, openai_compat).await {
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
    let model = {
        let cfg = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
        cfg.model.clone()
    };

    let schema_str = serde_json::to_string_pretty(schema).unwrap_or_default();
    let prompt_with_schema = format!(
        "{prompt}\n\n### OUTPUT FORMAT\n{tool_desc}. \
         Respond with ONLY a valid JSON object (no markdown fences, no prose) \
         matching this schema:\n{schema_str}"
    );

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

use super::types::{ToolDef, ToolResult};

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
    let (mock_llm, model, openai_compat) = {
        let cfg = app.llm_config.read().unwrap_or_else(|e| e.into_inner());
        (cfg.mock_llm, cfg.model.clone(), cfg.openai_compat)
    };
    if mock_llm {
        return Ok("Mock: ingest complete.".into());
    }

    let _permit = app.llm_semaphore.acquire().await
        .map_err(|e| anyhow::anyhow!("semaphore error: {e}"))?;

    let mut messages = vec![
        json!({"role": "system", "content": system}),
        json!({"role": "user", "content": user_msg}),
    ];

    for _iteration in 0..max_iterations {
        let body = build_multi_tool_request(
            &model, &messages, max_tokens, tools, openai_compat, reasoning_effort,
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

        if openai_compat {
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
        } else {
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
    openai_compat: bool,
    reasoning_effort: Option<&str>,
) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": messages,
    });

    if openai_compat {
        body["stream"] = json!(false);
        body["tools"] = json!(tools.iter().map(|t| json!({
            "type": "function",
            "function": {
                "name": t.name,
                "description": t.desc,
                "parameters": t.params,
            }
        })).collect::<Vec<_>>());
        body["tool_choice"] = json!("auto");
        if let Some(effort) = reasoning_effort {
            body["reasoning_effort"] = json!(effort);
        }
    } else {
        body["tools"] = json!(tools.iter().map(|t| json!({
            "name": t.name,
            "description": t.desc,
            "input_schema": t.params,
        })).collect::<Vec<_>>());
        body["tool_choice"] = json!({"type": "auto"});
        let effort = reasoning_effort.unwrap_or("high");
        body["reasoning"] = json!({"effort": effort});
    }

    body
}
