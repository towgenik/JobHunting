# AGENTS.md

**JobHunting** — Phase 1: scrape `jobstreet.co.id` job URLs, generate tailored CVs via LLM, queue for user approval. Single-user local tool. Rust + axum + HTMX + SQLite. Full design in `Architecture.md`; milestone list in `PLANS.md`.

This repo is worked by **two layers of agents** in parallel. Find your role below and follow its action card. Do not improvise across layers.

---

## If you are the Controller (operating at the project root, `JobHunting/`)

You orchestrate. **You never write project code.**

1. Read `.opencode/skills/orchestrate/SKILL.md` — your action card.
2. Read `main/PLANS.md` — find the lowest-numbered milestone with unchecked boxes.
3. Dispatch a worker for it (the orchestrate skill explains how).
4. Wait for the worker to print `READY: <slug> ready for merge` (or `BLOCKED: …`).
5. Integrate: merge into `main`, resolve conflicts per `Architecture.md` §8.5, remove the worktree, bump PLANS.md status.
6. Repeat for the next milestone.

You own: `.bare/`, `main/`, worktree lifecycle, conflict resolution, PLANS.md status on `main/`.

You do **not** own: any project source file, `.env`, the workers' in-progress branches.

---

## If you are a Worker (operating inside a `<slug>/` worktree)

You execute exactly one milestone. **You never merge.**

1. Read `.opencode/skills/worktree/SKILL.md` in your worktree — your action card.
2. Read `PLANS.md` in your worktree for your milestone's Verify and Done-when.
3. `make dev` to boot. Verify `curl -i localhost:3000/` → 200.
4. Do the milestone work. Commit as you go.
5. When Done-when criteria are met: update PLANS.md checkboxes + Status in your worktree, commit final.
6. Print exactly: `READY: <slug> ready for merge`
7. **Stop.** Do not merge, do not remove the worktree, do not start the next milestone.

If you cannot complete the milestone, print `BLOCKED: <slug> — <one-sentence reason>` and stop.

You own: files inside your `<slug>/` worktree, your branch's commit history.

You do **not** own: `main/`, other worktrees, the project root, merge decisions.

---

## Reference map

| Need to… | Read |
|----------|------|
| Start a milestone as a worker | `.opencode/skills/worktree/SKILL.md` |
| Dispatch a milestone as controller | `.opencode/skills/orchestrate/SKILL.md` (project root's `.opencode/`) |
| Understand the system | `Architecture.md` |
| See milestone status | `PLANS.md` |
| Dev cycle (request → response) | `Architecture.md` §7 |
| Multi-agent workspace rules | `Architecture.md` §8 |

---

## Hard rules (both roles)

- **Workers never run `git merge`, `git worktree remove`, or `git branch -d`.** Those are controller-only.
- **Controller never writes or edits project source files.** It manages worktrees and PLANS.md status.
- **Milestone order is strict** — see PLANS.md. Never skip ahead, never start M3 before M2's boxes are all checked on `main`.
- **One worktree per milestone, named after the slug** (`m2-scrape`, `m3-backend`). No speculative worktrees, no worktrees named after tools or people.
- **Never edit another agent's workspace.** Controller doesn't touch in-flight worktrees; workers don't touch other worktrees, `main/`, or the project root's `.opencode/`.
- **Never push to a remote** without explicit user instruction. Local-only until told.
- **Never edit `.env`** — that's the user's config.

---

## Workspace layout (for context)

```
JobHunting/                              ← controller operates here
├── .bare/                               ← bare hub (commit/branch storage)
├── .git                                 ← file: `gitdir: ./.bare`
├── .env                                 ← shared dev env (workers source via ../.env)
├── AGENTS.md                            ← symlink → main/AGENTS.md (this file)
├── .opencode/skills/orchestrate/        ← controller skill (machine-local)
├── main/                                ← integration worktree on `main`
│   ├── AGENTS.md                        ← canonical source (this file)
│   ├── Architecture.md, PLANS.md, …
│   └── .opencode/skills/worktree/       ← worker skill (tracked)
└── <slug>/                              ← per-milestone worker worktrees
    └── (copy of main/'s tracked files at branch point)
```
