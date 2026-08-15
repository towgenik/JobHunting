---
name: vision-analysis
description: |
  Send one or more images to any OpenAI-compatible vision model and get a
  textual answer back. One config file at ~/.agents/vision-analysis.json
  declares every (base_url, api_key) group + every model with its own
  context_length and max_output. Harness-agnostic: any agent that can run
  shell + read markdown can call the script directly. No SDK, no Node, no
  hardcoded URLs. Pillow required.
  Triggers: "vision", "analyze image", "look at image", "compare screenshots",
  "image to text", "describe screenshot", "vlm", "multimodal", "see image".
---

# vision-analysis

Pure-Python script that posts images to any OpenAI-compatible
`/chat/completions` endpoint. Drop into any LLM harness — agent shells,
test runners, CI, ad-hoc CLI — without modification.

## Layout

```
vision-analysis/
  SKILL.md                          ← this file
  scripts/
    analyze_vision.py               ← the only entry point (pure Python)
  vision-analysis.example.json      ← template → ~/.agents/vision-analysis.json

~/.agents/
  vision-analysis.json              ← ONE file: groups + settings (chmod 600)
```

Nothing else is shipped — no model registry, no hardcoded base URLs. Model
lineups and endpoints drift fast; the user is the source of truth.

## Setup (one-time)

### 1. Install Pillow

```bash
pip install Pillow          # or: uv pip install Pillow / pacman -S python-pillow
```

No other Python deps. urllib + json + base64 come from stdlib.

### 2. Copy template to `~/.agents/vision-analysis.json`

```bash
mkdir -p ~/.agents
cp .opencode/skills/vision-analysis/vision-analysis.example.json \
   ~/.agents/vision-analysis.json
chmod 600 ~/.agents/vision-analysis.json
$EDITOR ~/.agents/vision-analysis.json     # paste your api keys, prune groups you don't have
```

The script warns if the config file is group/world readable.

## Config schema

One file holds everything. Top-level keys:

| Key | Required | Holds |
|---|---|---|
| `groups` | **yes** | Map of `{group-name: {base_url, api_key, models}}` |
| `default_model` | **yes** | Model id used when `--model` is not passed |
| `fallback_models` | optional | Model ids tried in order on `fallback_on` errors |
| `fallback_on` | optional | Substrings triggering fallback (default: `["429", "timeout", "5xx", "network"]`) |
| `quality` | optional | JPEG quality (default 85) |
| `request_timeout` | optional | HTTP timeout seconds (default 120) |
| `thinking` | optional | Thinking-mode config block (default: off). See below |

Each **group** carries its own `base_url` + `api_key` and a `models` dict.
A group is the unit of "which endpoint am I hitting" — OpenRouter, Moonshot
direct, Gemini direct, DashScope, Zhipu, MiniMax, Together, Groq,
Fireworks, your local llama-server, etc. You define the groups you have
keys for. Multiple models can share one group.

```json
"groups": {
  "moonshot": {
    "base_url": "https://api.moonshot.cn/v1",
    "api_key":  "sk-...",
    "models": {
      "moonshot-v1-32k-vision-preview": {
        "context_length": 32768,
        "max_output":     2048,
        "max_dim":        4096
      },
      "moonshot-v1-8k-vision-preview": {
        "context_length": 8192,
        "max_output":     2048
      }
    }
  }
}
```

Each **model** inside a group has:

| Field | Required | Purpose |
|---|---|---|
| `context_length` | **yes** | Token budget for image scaling (script uses 80% of it) |
| `max_output` | optional | Sent as `max_tokens` in the request |
| `max_dim` | optional | Hard cap on the largest image dimension in pixels (e.g. Gemini 3072, Qwen 2048) |
| `thinking` | optional | Set to `true` to enable thinking/reasoning mode. **Default `false`** — see below |
| `api_key`, `base_url` | optional | Override the group-level values for this specific model |

**Per-endpoint key isolation is structural, not a feature flag.** Each
group has its own `api_key`. An OpenRouter key works only on
`openrouter.ai`; a Moonshot key works only on `api.moonshot.cn`; they
cannot be swapped. The fallback chain crosses groups naturally: when the
primary model fails, the next model is found in its own group, which
carries its own `api_key` + `base_url`.

## Thinking mode (default: off)

Vision is perception, not reasoning. Thinking / reasoning modes add
instability, timeouts, and empty outputs for typical vision tasks
(describe, compare, OCR). The script disables thinking by default. The
entire thinking config is in the JSON — both the default state and the
disabler payload:

```json
"thinking": {
  "enabled": false,
  "disable_payload": {
    "reasoning": {"enabled": false},
    "chat_template_kwargs": {"enable_thinking": false}
  }
}
```

- **`enabled`**: top-level default. Set `true` to flip every model to
  thinking mode unless overridden.
- **`disable_payload`**: dict merged into every request payload when
  thinking is off. Defaults cover OpenRouter / Anthropic (`reasoning`)
  and Qwen / vLLM (`chat_template_kwargs`). APIs that don't recognize a
  param silently ignore it per OpenAI-compat convention.

**Per-model override** — set `thinking: true` on a model to opt it back
in (overrides the top-level default):

```json
"models": {
  "anthropic/claude-3.7-sonnet:thinking": {
    "context_length": 200000,
    "max_output":     8192,
    "thinking":       true
  }
}
```

**Add a custom disabler** for an API the script doesn't know about —
just edit `disable_payload`:

```json
"thinking": {
  "enabled": false,
  "disable_payload": {
    "reasoning": {"enabled": false},
    "chat_template_kwargs": {"enable_thinking": false},
    "thinking": {"type": "disabled"}
  }
}
```

For models where thinking is hardcoded (DeepSeek R1, certain Qwen QwQ
builds), no payload param can disable it — pick the non-thinking variant
of the model id instead.

## How model lookup works

`find_model(model_id, config)` walks `groups` in order and returns the
first group whose `models` dict contains the id. That group's
`base_url`/`api_key` are used. There is no `provider` field on models, no
`family` field, no hardcoded `DEFAULT_BASE_URLS` to layer overrides on
top of. One lookup, one source of truth.

If a model id isn't in any group, the script errors out: you must
register it under exactly one group.

## How image scaling works

ONE function, no per-family dispatch. Pipeline:

1. **Fit to token budget** — uses 48px patch estimation, derived from
   the model's `context_length`. Larger context → larger allowed image.
2. **Cap to `max_dim`** — if the model declares a hard pixel limit
   (Gemini 3072, Qwen 2048, etc.), the image is downscaled to fit.

Both steps are optional and data-driven. A model with no `max_dim`
skips step 2. A model with a large `context_length` allows more tokens
in step 1. No model-family table, no `if family == "glm"` branches.

## Usage

```bash
# Default model (from config), one image
python3 scripts/analyze_vision.py "Describe this UI." screenshot.png

# Multiple images (e.g. before/after comparison)
python3 scripts/analyze_vision.py \
  "Compare these two screenshots. Report any visual differences." \
  before.png after.png

# Override model per-call (must be registered in some group)
python3 scripts/analyze_vision.py --model moonshot-v1-32k-vision-preview \
  "OCR this page." page.png

# Override resolved api_key per-call (CI, ephemeral key)
python3 scripts/analyze_vision.py --api-key sk-temp-... \
  "Quick check." ui.png

# Use a different config file (one-off, doesn't touch ~/.agents/)
python3 scripts/analyze_vision.py --config ~/work-vision.json \
  "Summarize chart." chart.png

# Disable fallback; fail on first error
python3 scripts/analyze_vision.py --no-fallback "Verdict?" ui.png

# Tune JPEG quality (default 85)
python3 scripts/analyze_vision.py --quality 95 "Read tiny text." scan.png
```

Output (stdout) is always:

```
--- Usage ---
Input: 1234 tokens | Output: 567 tokens | Total: 1801 tokens
--- Result ---
<model response text>
```

Diagnostics (image scaling, model/group selection, fallback transitions,
HTTP errors) go to stderr. Pipe stdout to capture just the answer.

## How to call from an LLM harness

This skill is **harness-agnostic**. One shell command, one prompt as
first arg, images as positional args.

### OpenCode (this project's default)

Already wired. The skill loader picks up this `SKILL.md`. The agent runs:

```bash
.opencode/skills/vision-analysis/scripts/analyze_vision.py "<question>" <img>...
```

### Codex CLI / Cursor / Claude Code / Aider / generic agent

Copy or symlink this folder into your repo (or `~/.skills/`), then have
the agent invoke the script directly. No harness-specific glue required.

```bash
./vision-analysis/scripts/analyze_vision.py "Question?" image.png
```

For harnesses that look for a single instruction file (Cursor rules,
`CLAUDE.md`, `AGENTS.md`), point them at this `SKILL.md`. The Usage
section above is the entire contract.

### Inline from Python / Node

```python
import subprocess, pathlib
script = pathlib.Path(".opencode/skills/vision-analysis/scripts/analyze_vision.py")
out = subprocess.check_output(
    ["python3", str(script), "Describe this.", "ui.png"],
    text=True,
)
print(out.split("--- Result ---\n", 1)[1])
```

## Tips

- **Always pass two images when comparing.** Same question, both files;
  the model handles the diff.
- **State ground truth in the question.** "Background should be white,
  text dark, images unchanged" beats "is this correct?".
- **Register every model you want to call.** The script refuses unknown
  model ids rather than guessing which group they belong to — explicit
  is safer than heuristic.
- **One group per (base_url, api_key) pair.** If you have two OpenRouter
  accounts, name them `openrouter-personal` and `openrouter-work`, not
  two `openrouter` entries.
- **Use `max_dim` for APIs with hard pixel caps.** Gemini 3072, Qwen
  2048, GLM 4096. Without it the script falls back to token-budget-only
  fitting, which can overshoot pixel limits on large screenshots.
- **Phrase questions directly, not as reasoning prompts.** "Describe the
  layout of this UI" beats "Let's think step by step about this UI".
  Thinking-mode prompts bias the model toward text-prior hallucination
  even when thinking params are disabled.

## What this skill does NOT do

- No streaming (one-shot completion only).
- No retries with backoff — one fallback attempt per model in the chain,
  then fail. Wrap with `retry` from your shell if you need more.
- No image generation, only analysis.
- No batched / async / multi-turn conversations.
- No hardcoded base URLs. If you want to call a new endpoint, add a
  group with its `base_url` and `api_key`.
- No encryption at rest — `chmod 600` is the only secret protection.
  For stronger guarantees, point `--config` at a file mounted from an OS
  keyring, FUSE secret store, or `pass`.

## Backward compatibility

| Old | New |
|---|---|
| `bash scripts/analyze_vision.sh "<q>" <imgs>` | `python3 scripts/analyze_vision.py "<q>" <imgs>` |
| `VISION_API_KEY` / `VISION_BASE_URL` / `VISION_MODEL` env vars | Still work (deprecated, warns). Move to `~/.agents/vision-analysis.json` for production. |
| `.env` next to skill | Removed. |
| `providers.<name>` config (flat) | Replaced by `groups.<name>` with embedded `models`. |
| `DEFAULT_BASE_URLS` (hardcoded in script) | Removed. Every `base_url` is user-declared per group. |
| `family` field on models (`gemma` / `glm` / `qwen` / ...) | Removed. Single scaling pipeline, configured via optional `max_dim`. |
| Separate `models` registry file | Removed. Models live inside their group in the same config file. |
