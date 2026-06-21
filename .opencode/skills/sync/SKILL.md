---
name: sync
description: >
  Keep PLANS.md, Architecture.md, and project skills in sync after milestone
  work. Run before every READY or BLOCKED signal. Captures lessons as skills
  so future agents start smarter.
---

# Doc Sync + Self-Improvement

Run before every READY or BLOCKED. This is how the project gets smarter with
each milestone instead of repeating the same mistakes.

## 1. PLANS.md (always)

- Check every completed box: `- [x]`
- Update the Status line: `M<N> complete — M<N+1> next`
- If you stayed BLOCKED: note the blocker in the Status line so the controller
  knows what to resolve before redispatching.

## 2. Architecture.md (when implementation diverged from spec)

The spec must match what's actually built. A lying spec is worse than no spec
— the next agent will implement the lie.

Ask: *"Does Architecture.md now describe what I actually shipped?"*

If no → fix the relevant section (§4 scraper, §5 Rust, §6 templates, etc.)
before committing. Scope: only the section your milestone touched.

Do NOT update Architecture.md for code that doesn't exist yet — spec-ahead of
code is fine; spec-behind code is not.

## 3. Create a skill (when a future agent would waste time without it)

A skill earns its place when it captures:
- A failure you spent >10 min diagnosing (third-party quirk, env issue, silent
  misconfiguration, wrong docs)
- A better approach than Architecture.md described — only after Architecture
  has been updated to match
- A tool/library behavior that surprised you and isn't obvious from its docs

Write it at `.opencode/skills/<component>/SKILL.md`. Name after the component,
not the milestone: `.opencode/skills/scrapling-session/` not
`.opencode/skills/m2-lessons/`. Commit it — it propagates to every future
worktree via `git worktree add … main`.

**When NOT to create a skill:**
- Already documented in Architecture.md
- A one-off (typo, missing env var you just added, unrepeatable env issue)
- Obvious from the code or standard docs

**Skill template:**
```markdown
---
name: <component>
description: <one line — what this is about>
---

# <Component> — <what was learned>

**Problem:** what broke or surprised you
**Root cause:** why it happened
**Fix:** what works
**ponytail:** ceiling + upgrade path if this is a known shortcut
```

## 4. Self-improvement loop

**BLOCKED path:**
1. Write a skill: what blocked you, what you tried, what would unblock it.
2. Update PLANS.md Status with the blocker summary.
3. Signal BLOCKED. Controller reads the skill before redispatching.

**READY path:**
1. Write a skill only if rule 3 above applies.
2. If everything went as Architecture.md predicted — no skill needed (YAGNI).
3. Update docs (rules 1–2), signal READY.

## 5. Add new skills to AGENTS.md

When you create a skill, add one row to AGENTS.md's reference map:
```
| <When to read this> | `.opencode/skills/<name>/SKILL.md` |
```
Then verify the cap: `test $(wc -c < AGENTS.md) -lt 4096 && echo OK`

If over cap, trim a less-useful row first (prefer rows that duplicate
Architecture.md over rows that point to unique skills).
