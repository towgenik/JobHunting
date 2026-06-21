# PLANS.md

## Goal

Phase 1: paste a `jobstreet.co.id` job URL → scrape → LLM-tailored CV → user approves/rejects. One site, end-to-end, no scaffolding for the other 42.

**Status:** Workspace restructured for multi-agent work (bare+worktree, see Architecture §8). M1 complete — repo bootstraps, migration runs, `make dev` serves 200 on :3000 from `main/` worktree. M2 (scrape spike) next.

Execution order is strict. Each milestone is verifiable before the next starts. Don't skip ahead; an unverified milestone rots.

---

## Workspace (per-milestone rhythm)

Each milestone runs in its own worktree so multiple agents don't collide. Pattern is documented in Architecture §8; the short version:

```bash
# Start a milestone (from project root)
git worktree add m2-scrape -b m2-scrape main
cd m2-scrape
make dev                                  # sources ../.env, builds in local target/

# … do the work, verify …

# Finish: merge from the main worktree
cd ../main && git merge m2-scrape
git worktree remove ../m2-scrape && git branch -d m2-scrape

# Publish (controller only, on user instruction — Architecture §8.7)
git push origin main
```

Shared state lives at the project root (`.env`); per-worktree state (`target/`, `jobagent.db`) is isolated and disposable. GitHub remote (`origin`) is private at `menggatot/JobHunting`; only the controller pushes, only from `main/`.

---

## Milestones

### M1 — Repo bootstrap ✅
- [x] `git init` (repo not yet initialized)
- [x] `cargo init`, drop in `Cargo.toml` from Architecture §5.1
- [x] `Makefile` from §2.2 with one change: `dev` target sources `.env` before `cargo watch` (e.g. `set -a; source .env; set +a; exec cargo watch -x run`). Architecture §5.2 uses `std::env::var().expect()` — without this, boot panics. No `dotenvy` crate; fewer deps. *(Note: used POSIX `. ./.env` instead of `source` — `/bin/sh` on this host is dash.)*
- [x] `.gitignore` (`/target`, `*.db*`, `.env`), `.env.example` (all 5 vars from §6.6)
- [x] `migrations/0001_init.sql` from §3 **plus** a `reject_reason TEXT` column on `jobs` (known-needed by M5; folding now avoids a second migration)
- [x] `src/main.rs` boots an empty axum router on `127.0.0.1:3000`
- **Verified:** `make dev` then `curl -i localhost:3000/` → `200 OK`, body `ok`.
- **Done:** server boots clean, migration runs, `.env` loads, repo committed (`178770a`).

### M2 — Scrape spike (standalone, before any Rust)
- [ ] `command -v brave-browser` — fail fast with an install hint if missing (Brave is the only runtime we don't control)
- [ ] `scrape.py` from §4, runnable on its own
- [ ] Run against **3 real** JobStreet job-detail URLs (different categories if possible)
- [ ] Inspect `{"title","description"}` JSON on stdout — both fields non-empty for all 3
- [ ] If any field is empty, tune selectors and **update Architecture §4** to match what actually works
- **Verify:** `python scrape.py <url>` prints valid JSON three times in a row.
- **Done when:** 3/3 sample URLs return non-empty title + description. No Rust touched yet.
- **Why first:** site HTML is the only piece we don't control. Don't wire a scraper you haven't seen print JSON.

### M3 — Full backend in one pass (DB + templates + pipeline + mock LLM)
*v1 plan's M3+M4 merged — `cargo check` alone proves nothing; the only real verification is the end-to-end happy path.*

- [ ] `src/db.rs`: every function referenced in §5 (`create_job_stub`, `set_status`, `get_job_url`, `update_job_data`, `get_status`, `save_cv_draft`, `get_master_cv`, `render_cv_ready`, settings get/upsert). Plain `sqlx::query` — no compile-time macro dance unless a typo bites.
- [ ] `src/templates.rs` + `templates/`: askama structs from §5.3; HTML from §6 (base, index, job, settings, fragments/processing, fragments/cv_ready). Dashboard input includes the `pattern="…jobstreet.co.id/…"` guard from §6.2.
- [ ] `src/generate.rs`: `process_job`, `fetch_job`, `scrape_once`, `build_prompt`, `call_llm` (mock branch only).
- [ ] Routes in `src/main.rs`: `POST /jobs` (with `is_phase1_url` host guard), `GET /jobs/:id/card`, `GET /`.
- [ ] HTMX polling card lifecycle: processing → cv_ready (stops polling).
- **Verify:** with `LLM_MOCK=true make dev`, paste a real JobStreet URL → polling card appears → resolves to "CV ready" within ~15s. Inspect DB row: status ends at `pending_approval`, `cv` column is non-empty.
- **Done when:** full happy path works offline against mock LLM. Templates, DB, and pipeline all proven at once.

### M4 — Real LLM call
- [ ] Implement non-mock branch of `call_llm` against `LLM_ENDPOINT`
- [ ] Confirm JSON shape matches the §5.6 schema (summary / skills / experiences)
- **Verify:** unset `LLM_MOCK`, repeat M3's flow → CV content is clearly derived from the job description, not the mock.
- **Done when:** same flow as M3 produces a real tailored CV.

### M5 — Review + decision (`GET /jobs/:id`, `POST /jobs/:id/decision`)
- [ ] Two-panel CV review page from §6.4
- [ ] Approve → status `approved`. Reject → status `rejected`, reason stored in `reject_reason` (column added in M1, no new migration).
- **Verify:** approve and reject both work; reason persists across page reload.
- **Done when:** decision flow is fully clickable and persists.

### M6 — Settings (master CV editor)
- [ ] `GET /settings`, `POST /settings` (upsert on single-row table)
- [ ] Confirm `process_job` reads the saved master CV into the prompt
- **Verify:** save master CV → run another job → generated CV references skills/experiences from the master.
- **Done when:** master CV feeds generation.

### M7 — Hardening pass (only what's actually bitten)
- [ ] Scrape failure → status `failed` → card shows error.
- [ ] Duplicate URL → return the existing row's card (feature, not error: a UNIQUE collision means the user already pasted this URL — surface it, don't 500). Update Architecture §5.7 when this lands.
- [ ] One `assert`-based self-check per non-trivial module per the ponytail rule — `generate.rs` and `db.rs` get a `demo()` or `#[test]`. Nothing else unless observed.
- [ ] Anything else only if observed during M1–M6 — no speculative error handling.
- **Verify:** 10-URL test session produces no unhandled panics in `cargo run`.
- **Done when:** Phase 1 exit criteria from Architecture §1.1 met — 10 sample JobStreet URLs flow through the full UI without manual intervention.

---

## Explicitly deferred

- **Other 42 sites** → Phase 2, one at a time, ordered by xlsx category
- **Per-site selector config map** → Phase 2, when the second site is attempted
- **Listing/index crawler** → Phase 3, only if per-URL proves insufficient
- **Per-function test suites / framework** → only if M7's self-checks prove insufficient
- **Auth, multi-user, deployment** → never; this is a single-user local tool
