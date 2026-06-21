---
name: anthropic-api-gotchas
description: Three gotchas when calling the Anthropic messages API directly via reqwest (raw HTTP, not the Python/TS SDK)
---

# Anthropic Messages API — Raw HTTP Gotchas (M4)

## Gotcha 1: `x-api-key` header, not Bearer auth

**Problem:** The Architecture spec said `.bearer_auth(&app.llm_api_key)` (i.e. `Authorization: Bearer <key>`). This returns a 401.

**Root cause:** Anthropic's API uses a custom `x-api-key` header, not the OAuth Bearer pattern. Bearer auth is only for OAuth tokens (used with `ant auth login`, not API keys).

**Fix:**
```rust
.header("x-api-key", &app.llm_api_key)
.header("anthropic-version", "2023-06-01")
```

**ponytail:** The `anthropic-version` header is also required. Without it you get a 400.

---

## Gotcha 2: Request body is `messages` array, not `{ model, prompt }`

**Problem:** The spec had `json!({ "model": app.llm_model, "prompt": prompt })`. This 400s.

**Root cause:** Anthropic uses the Messages API format — `messages` is a list of `{role, content}` objects. There is no top-level `prompt` field.

**Fix:**
```rust
let body = json!({
    "model": app.llm_model,
    "max_tokens": 2048,
    "messages": [{"role": "user", "content": prompt}]
});
```

**ponytail:** `max_tokens` is required. Without it the API returns 400.

---

## Gotcha 3: Response text is at `content[0].text`, not the top-level object

**Problem:** Returning `resp` directly gives the full Anthropic response envelope — not the CV JSON the prompt asked for.

**Root cause:** Anthropic wraps the model's output in `response["content"][0]["text"]` (a string). The prompt asks for JSON output, so that string must be parsed with `serde_json::from_str`.

**Fix:**
```rust
let text = resp.pointer("/content/0/text")
    .and_then(|v| v.as_str())
    .ok_or_else(|| anyhow::anyhow!("missing content[0].text: {resp}"))?;
let cv: Value = serde_json::from_str(text)?;
```

**ponytail:** Check `status.is_success()` on the HTTP response *before* parsing JSON, because error responses from Anthropic (4xx/5xx) return `{ "type": "error", "error": { "type": "...", "message": "..." } }` — valid JSON but not the shape you want to parse as a CV.
