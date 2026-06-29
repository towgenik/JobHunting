---
name: sync
description: >
  Keep AGENTS.md and project skills in sync after milestone work.
  Run before every READY or BLOCKED signal. Captures lessons as skills
  so future agents start smarter.
---

# Doc Sync + Self-Improvement

Run before every READY or BLOCKED. This is how the project gets smarter with
each milestone instead of repeating the same mistakes.

## 1. AGENTS.md (always)

- Verify AGENTS.md still reflects the current architecture.
- If your milestone changed the stack, added new modules, or changed the
  pipeline — update AGENTS.md accordingly.
- Keep it under 4KB — trim less-useful rows before adding new ones.

## 2. Create a skill (when a future agent would waste time without it)

A skill earns its place when it captures:
- A failure you spent >10 min diagnosing (third-party quirk, env issue, silent
  misconfiguration, wrong docs)
- A tool/library behavior that surprised you and isn't obvious from its docs

Write it at `.opencode/skills/<component>/SKILL.md`. Name after the component,
not the milestone: `.opencode/skills/scrapling-session/` not
`.opencode/skills/m2-lessons/`. Commit it — it propagates to every future
worktree via `git worktree add … main`.

**When NOT to create a skill:**
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

## 3. Self-improvement loop

**BLOCKED path:**
1. Write a skill: what blocked you, what you tried, what would unblock it.
2. Update PLANS.md Status with the blocker summary.
3. Signal BLOCKED. Controller reads the skill before redispatching.

**READY path:**
1. Write a skill only if rule 2 above applies.
2. Update docs (rule 1), signal READY.

## 4. Add new skills to AGENTS.md

When you create a skill, add one row to AGENTS.md's reference map:
```
| <When to read this> | `.opencode/skills/<name>/SKILL.md` |
```
Then verify the cap: `test $(wc -c < AGENTS.md) -lt 4096 && echo OK`
