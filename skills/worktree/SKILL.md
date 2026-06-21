---
name: worktree
description: >
  Guide agents working on the JobHunting project to claim an isolated git
  worktree before writing any code. Use whenever an agent starts work on a
  milestone in PLANS.md, when an agent says "I'll work on M2" / "picking up
  the scrape spike" / similar, or whenever an agent is about to edit project
  files. Prevents multiple agents from colliding in one working directory.
license: MIT
---

# Worktree Workflow (JobHunting)

Bare hub + worktree layout. Full rationale in `Architecture.md` §8; this is
the action card. **Read this before editing any file in this repo.**

## Layout (already set up — don't recreate)

```
JobHunting/                  ← project root; no working files live here
├── .bare/                   ← bare hub (commit/branch storage)
├── .git                     ← file: `gitdir: ./.bare` (git works from root)
├── .env                     ← shared dev env (gitignored)
├── main/                    ← integration worktree on `main` — don't edit directly
└── <slug>/                  ← your worktree (you create one per milestone)
```

## Start a milestone

From the project root (`JobHunting/`):

```bash
git worktree add <slug> -b <slug> main   # slug e.g. m2-scrape, m3-backend
cd <slug>
make dev                                 # sources ../.env; first build ~1min, after ~0.3s
```

Then in another shell: `curl -i localhost:3000/` → expect `200 OK` body `ok`.
Stop the server (Ctrl-C) once verified. Now do the work.

Commit normally inside the worktree — the bare hub sees your commits
immediately. No `git push` between worktrees, ever.

## Finish a milestone

Only when the milestone's "Done when" criteria in `PLANS.md` are met:

```bash
cd ../main
git merge <slug>
git worktree remove ../<slug>
git branch -d <slug>
```

Then in `main/`: update PLANS.md checkboxes and the Status line, commit,
that's the integration commit.

## Hard rules

- **Never edit `main/`** except post-merge bookkeeping (PLANS.md status).
- **Never edit other worktrees** — they belong to other agents.
- **Never push between worktrees.** Merging via `main` is the only path.
- **Never create worktrees speculatively.** Create at milestone start, remove
  at milestone end. No `m5-…` dirs lying around for work that hasn't begun.
- **Never reconfigure paths.** `../.env` shared, `target/` per-worktree,
  `jobagent.db` per-worktree — these are correct (Architecture §8.5).
- **Never run `make dev` from the project root** — only from inside a
  worktree.

## Self-check before merging

- [ ] `make dev` boots clean in your worktree
- [ ] PLANS.md "Verify" step for this milestone passes
- [ ] `git status` clean in your worktree
- [ ] No edits leaked outside your worktree (`cd ../main && git status`
      shows nothing caused by you)
- [ ] PLANS.md checkboxes + Status line updated in `main/`

If any box unchecked, **do not merge**.

## Anti-patterns to refuse

If you (the agent) notice yourself doing any of these, stop and re-read this
skill:

- Editing files in `main/` because "it's faster" — no it isn't, it collides.
- Sharing `target/` via symlink "to save build time" — concurrent builds
  corrupt it.
- Creating a worktree named after a person/tool instead of the milestone —
  name = milestone slug, always.
- Starting M3 before M2's "Done when" — milestones are strictly ordered
  (PLANS.md).
