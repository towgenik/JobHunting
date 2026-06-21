---
name: worktree
description: >
  Guide agents working on the JobHunting project to claim an isolated git
  worktree before writing any code. Use whenever an agent is dispatched to
  work on a milestone in PLANS.md, when an agent says "I'll work on M2" /
  "picking up the scrape spike" / similar, or whenever an agent is about to
  edit project files. Prevents multiple agents from colliding in one
  working directory. Workers never merge — they signal READY and the
  controller at the project root integrates.
license: MIT
---

# Worktree Workflow (JobHunting Worker)

You are a worker. You live inside one worktree (`<slug>/`) and own exactly
that directory for exactly one milestone. The controller at the project root
dispatched you and will integrate your work — **you do not merge, you do not
remove your worktree.**

Full rationale in `Architecture.md` §8; this is the action card.

## Layout (your workspace)

```
JobHunting/                  ← project root; the controller lives here
├── .bare/                   ← bare hub (don't touch directly)
├── .env                     ← shared dev env you source via ../.env
├── main/                    ← integration branch; don't edit directly
├── .opencode/skills/        ← controller's skills; not yours
└── <slug>/                  ← YOUR worktree (you were spawned here)
    └── .opencode/skills/worktree/SKILL.md   ← this file
```

## Start (you've already been dispatched)

If you're reading this, the controller already created your worktree. Verify
boot, then work:

```bash
make dev                                  # sources ../.env; first build ~1min
```

In another shell: `curl -i localhost:3000/` → expect `200 OK`. Stop the
server once verified. Now do the milestone work.

Commit normally inside the worktree — the bare hub sees your commits
immediately. No `git push`, no merge, ever.

## Signal READY when done

When your milestone's "Done when" criteria in `PLANS.md` are met:

1. Update `PLANS.md` in your worktree:
   - Check the milestone's boxes (`- [x]`)
   - Bump the Status line (e.g. "M2 complete — M3 next")

2. Commit everything:
   ```bash
   git add -A
   git commit -m "<slug> complete"
   ```

3. Print this exact line as your final output so the controller can detect
   it:
   ```
   READY: <slug> ready for merge
   ```

4. **STOP.** Do not merge. Do not remove your worktree. Do not start the
   next milestone. Wait for the controller to integrate.

## Signal BLOCKED if you can't finish

If you cannot complete the milestone (verify step won't pass, blockers
outside your scope, missing prerequisites), print:
```
BLOCKED: <slug> — <one-sentence reason>
```
and stop. The controller will escalate or send you back with guidance.

## Hard rules

- **Stay in your worktree.** Never edit `main/`, never edit other worktrees,
  never edit the project root's `.opencode/`.
- **Never run `git merge`, `git worktree remove`, or `git branch -d`.** Those
  are the controller's. You commit; you don't integrate.
- **Never push to a remote.** Local-only.
- **Never reconfigure paths.** `../.env` shared, `target/` per-worktree,
  `jobagent.db` per-worktree — these are correct (Architecture §8.5).
- **Never edit `.env`** — that's the user's config.
- **Never refine the controller's skills** (`.opencode/skills/orchestrate/`
  at project root). That's not your layer.
- **You MAY refine this skill** (`.opencode/skills/worktree/SKILL.md` in your
  worktree) if you learned something useful — the controller will merge it.

## Self-check before signaling READY

- [ ] `make dev` boots clean in your worktree
- [ ] PLANS.md "Verify" step for this milestone passes
- [ ] PLANS.md "Done when" criteria actually met (don't self-deceive)
- [ ] `git status` clean
- [ ] No edits leaked outside your worktree (`cd ../main && git status`
      shows nothing caused by you)
- [ ] PLANS.md checkboxes + Status line updated in your worktree
- [ ] No secrets tracked:
      ```bash
      git ls-files | grep -iE '\.(env|pem|key|p12)$|secrets/|\.db[^/]*$'
      ```
      Output must be empty. If a secret slipped in, escalate to user — never scrub history silently.

If any unchecked, **do not signal READY**.

## Anti-patterns to refuse

If you notice yourself doing any of these, stop and re-read this skill:

- Merging your own branch to main — that's the controller's job.
- Editing `main/` because "it's faster" — it breaks isolation.
- Sharing `target/` via symlink "to save build time" — concurrent builds
  corrupt it.
- Starting the next milestone without the controller dispatching you —
  PLANS.md order is strict.
- Signaling READY with uncommitted changes — `git status` must be clean.
