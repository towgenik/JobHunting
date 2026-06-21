---
name: deepseek-prompt-fidelity
description: >
  Two prompt-engineering gotchas hit during M9 when generating CV JSON via
  DeepSeek (OpenRouter, OpenAI-compatible mode): silent empty-array omissions
  and silent mid-JSON truncation. Read before tuning build_prompt or bumping
  max_tokens for any OpenAI-compatible provider.
---

# DeepSeek (OpenRouter) — prompt fidelity + truncation gotchas

Hit during M9. The CV generator calls DeepSeek via OpenRouter using the
OpenAI-compatible chat-completions API. Two unrelated silent failures both
produced incomplete CVs.

## Problem 1: silent empty-array omission

**Symptom:** the LLM returned a valid CV JSON with `summary` + `skills` filled
in, but `experiences: []` — the array was syntactically valid, just empty. No
error, no warning. The template rendered a CV with no work history.

**Root cause:** the original schema wording was suggestive, not prescriptive:

```jsonc
"experiences": [{"company": "String", "role": "String",
                 "bullet_points": ["achievement-focused, quantified"]}]
```

DeepSeek read this as a *type* description ("if you include experiences, here
is the shape") rather than a *requirement* ("you must include experiences").
When the master CV's work history didn't map cleanly onto the JD, the model
took the conservative path and emitted `[]` rather than fabricating.

**Fix:** three reinforcements, all needed — any one alone still leaked:

1. **TASK text spells out the hard requirement** with the failure mode named:
   "`experiences` MUST contain AT LEAST ONE (≥1) entry — an empty array is a
   FAILURE." Tell the model what to do *and* what not to do.
2. **Schema uses min-length language** ("REQUIRED, MIN LENGTH 1 — never []")
   and describes each field on its own line. Loose one-line shapes get
   interpreted as optional.
3. **One-shot example** — a fully-populated JSON object under an
   `### EXAMPLE OUTPUT` heading. The model copies the *shape* (array of
   objects with company/role/bullet_points) and replaces the *content* with
   JD-derived material. This is the single highest-leverage change.

Add a trailing reminder after the example too: "`experiences` MUST contain at
least one entry." Repetition at the end of a long prompt matters — models
weight the last instructions heavily.

## Problem 2: silent mid-JSON truncation (the misleading error)

**Symptom:** after the M9 prompt change, two of three sample URLs failed with
`error decoding response body` from reqwest. The third succeeded. Same code,
same provider — looked transient.

**Root cause:** the stricter M9 prompt (explicit fields + example + ≥1
experience requirement) makes DeepSeek emit *longer, more thorough* CVs. The
hardcoded `max_tokens: 2048` was now too small. The response got cut off
mid-string or mid-array. The actual serde_json error was `EOF while parsing a
string at line 9 column 88` and `EOF while parsing a list at line 18 column 0`
— but reqwest's `.json()` call collapses this to the unhelpful
`error decoding response body` because it can't tell HTTP-body-decode failure
from JSON-parse failure.

**Fix:** `max_tokens: 2048 → 4096`. 4096 leaves headroom for 3 experiences ×
5 bullets without being wasteful.

**Diagnostic tip (do this before assuming "transient LLM error"):** when
`call_llm` fails, grep the server log for the underlying serde error — it
appears after the colon in `LLM returned non-JSON text: <serde error>`. If you
see `EOF while parsing`, it's truncation, not a flaky API. Bump `max_tokens`.
The bare `error decoding response body` only shows up when the body itself
isn't even valid JSON at the HTTP layer (e.g. an HTML error page from a
proxy); the serde errors show up when the body is well-formed HTTP but the
JSON is incomplete.

## ponytail

- The `max_tokens` ceiling is a known shortcut — the real fix would compute
  token budget from the JD + master_cv length. 4096 is fine for Phase 1;
  revisit if very long JDs start truncating again.
- The empty-array fix is *defensive prompting*, not a schema guarantee. A
  proper fix would use OpenAI's structured outputs / JSON schema enforcement,
  but DeepSeek via OpenRouter doesn't expose that. The prompt-level fix is
  the right altitude for Phase 1.
