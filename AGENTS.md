# AGENTS.md

**JobHunting** — Phase 1: scrape `jobstreet.co.id` job URLs, generate tailored CVs via LLM, queue for user approval. Single-user local tool. Rust + axum + HTMX + SQLite. Full design in `Architecture.md`; milestones in `PLANS.md`.

Two-layer agent repo. Find your role below, follow its action card.

## Modification charter

**Any agent may edit this file** — controller or worker. The cap is **4 KB / ~1000 tokens**, checked with `wc -c AGENTS.md` (must report `< 4096`). The point is to keep AGENTS.md loadable in one glance; depth lives in skills.

**When content doesn't fit:** extract it into a skill at `.opencode/skills/<name>/SKILL.md` (controller skills go in the project-root `.opencode/`; worker skills go in the worktree's `.opencode/`), then leave a **one-line pointer** in AGENTS.md's reference map. Prefer tight + pointed over complete + long.

**Edit flow:** workers edit in their worktree and merge via the normal `Architecture.md` §8.4–§8.5 flow; controller edits on `main` directly. Conflict resolution (`Architecture.md` §8.5): prefer the tighter version that points to a skill over the longer version that inlines detail.

## If you are the Controller (at project root, `JobHunting/`)

You orchestrate. **You never write project code.**

1. Read `.opencode/skills/orchestrate/SKILL.md`.
2. Read `main/PLANS.md` — find the lowest-numbered milestone with unchecked boxes.
3. Dispatch a worker for it.
4. Wait for `READY: <slug> ready for merge` (or `BLOCKED: <slug> — <reason>`).
5. Integrate: merge into `main`, resolve per §8.5, remove worktree, bump PLANS.md status.
6. Repeat.

Owns: `.bare/`, `main/`, worktree lifecycle, conflict resolution, PLANS.md status on `main/`.
Does **not** own: project source files, `.env`, in-flight worker branches.

## If you are a Worker (in a `<slug>/` worktree)

You execute one milestone. **You never merge.**

1. Read `.opencode/skills/worktree/SKILL.md` in your worktree.
2. Read `PLANS.md` for your milestone's Verify and Done-when.
3. `make dev` to boot. Verify `curl -i localhost:3000/` → 200.
4. Do the work. Commit as you go.
5. Done-when met → update PLANS.md checkboxes + Status in your worktree, commit.
6. Print `READY: <slug> ready for merge` and stop.

Cannot finish? Print `BLOCKED: <slug> — <one-sentence reason>` and stop.

Owns: files inside `<slug>/` worktree, your branch's commit history.
Does **not** own: `main/`, other worktrees, project root, merge decisions.

## Reference map

| Need to… | Read |
|----------|------|
| Start a milestone (worker) | `.opencode/skills/worktree/SKILL.md` |
| Dispatch a milestone (controller) | `.opencode/skills/orchestrate/SKILL.md` |
| Understand the system | `Architecture.md` |
| Milestone status | `PLANS.md` |
| Sync docs + create skills (before READY/BLOCKED) | `.opencode/skills/sync/SKILL.md` |

## Hard rules (both roles)

- **Workers never run `git merge`, `git worktree remove`, `git branch -d`.** Controller-only.
- **Controller never writes or edits project source files.**
- **Milestone order is strict** — see PLANS.md.
- **One worktree per milestone**, slug-named (`m2-scrape`, `m3-backend`).
- **Never edit another agent's workspace** — controller skips in-flight worktrees; workers skip other worktrees and `main/`.
- **Never push to a remote** without explicit user instruction.
- **Never edit `.env`** — user's config.
- **Never commit secrets or build trash.** `.gitignore` covers `/target`, `*.db*`, `.env*`, `*.pem`, `*.key`, `*.p12`, `secrets/`, `.venv/`, `__pycache__/`, `*.log`. New artifact type → extend `.gitignore` in the same commit. Pre-READY check: `git ls-files | grep -iE '\.(env|pem|key|p12)$|secrets/|\.db[^/]*$'` must be empty. If a secret slipped in, escalate to user — never scrub history silently.

## Self-check before commit (any agent)

```bash
test $(wc -c < AGENTS.md) -lt 4096 && echo OK || echo "AGENTS.md too fat — extract to a skill"
git ls-files | grep -iE '\.(env|pem|key|p12)$|secrets/|\.db[^/]*$' && echo "FAIL: secrets tracked" || echo OK
```
