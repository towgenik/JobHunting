# PLANS.md

## Goal

Phase 1: paste an `id.jobstreet.com` job URL → scrape → LLM-tailored CV → user approves/rejects. One site, end-to-end, no scaffolding for the other 42.

**Status:** M10 complete — M11 next. Phase 1 ✅ + containerized deploy ✅ proven end-to-end (fresh-volume `docker compose up` → paste JobStreet URL → real LLM CV with non-empty experiences, all in-container). M10 fixed three container-only bugs (glibc mismatch, `create_if_missing` default, stale-name/port conflicts) — see `docker-multistage` skill. Next: **M11** (CD pipeline verify).

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

### M1 — Workspace bootstrap ✅ (complete; pre-workflow setup)

One-time setup, completed before the multi-agent workflow existed. Git history holds the detail (commits `178770a` → `ea17e6c`). What's in place **now**, and what an agent joining at M2 must know:

**Layout (see Architecture §8.1, §8.8):**
- `.bare/` hub + `.git` file at project root pointing at it
- `main/` worktree on `main` branch — the integration surface
- `.env` at project root (shared by all worktrees via `../.env`)

**Code in `main/`:**
- `Cargo.toml` (deps from Architecture §5.1)
- `Makefile` (sources `../.env`; runs `cargo watch -x run`)
- `migrations/0001_init.sql` (schema from §3 + `reject_reason` folded in)
- `src/main.rs` (empty axum router on `127.0.0.1:3000`)
- `.gitignore`, `.env.example`

**Agent layer:**
- `AGENTS.md` — mutable entry doc, 4 KB cap
- `.opencode/skills/worktree/SKILL.md` — worker action card (committed; propagates to new worktrees)
- `.opencode/skills/orchestrate/SKILL.md` at **project root** (machine-local; not in any worktree)
- `.pi/skills`, `.claude/skills` symlinks → `.opencode/skills`

**Remote:** `origin → github.com:menggatot/JobHunting.git` (private). Controller pushes only, from `main/`, on user instruction (§8.7).

**Verify (run from `main/`):**
```bash
make dev                                  # ~0.3s boot; sources ../.env
curl -i localhost:3000/                    # → 200 OK, body 'ok'
git log --oneline                          # 10 commits, latest 'ea17e6c'
git -C ../.bare remote -v                  # origin → github.com:menggatot/JobHunting.git
git worktree list                          # .bare (bare) + main on main
```

**Done.** Workspace ready; agents can be dispatched on M2+. **M2+ follow the per-worktree rhythm** (Workspace section above, Architecture §8.3–§8.4, worker skill). The M1 commands above don't apply to feature milestones — they were one-time setup.

### Prep — container & CI scaffolding (before M2; "prep the kitchen")

Foundation so M2–M7 can cook and the result deploys anywhere. Direct edits in `main/` (not a dispatched milestone).

- [x] Root `docker-compose.yml` — portable `login` service (de-host-coupled); `app` stubbed for M8 (Architecture §9)
- [x] Native `login/` image — renamed from `kasmvnc-docker/`, de-host-coupled (no host networking, no `/home/user/...` mounts), host-specific docs removed; only `Dockerfile` + `entrypoint.sh` remain
- [x] `.github/workflows/ci.yml` — self-hosted runner; `cargo check` + `cargo test` + project self-checks (Architecture §9.3)
- [x] `.opencode/skills/sync/SKILL.md` — doc-sync + self-improvement protocol; agents write skills on failures/surprises before every READY
- [x] **Done:** committed + pushed — `login/`, `docker-compose.yml`, `.github/`, `.opencode/skills/sync/` all on `main` (`7e5d23e`)
- [x] **Done:** `docker compose up login` → noVNC :6901 returns `401` (auth prompt) ✓, CDP :9223 returns Chrome JSON ✓ — bridge networking works
- [x] **Done:** self-hosted runner `jobhunting-runner` registered + running as systemd service; CI green ✓
- **Verify:** CI green ✓ (`7e5d23e`). `docker compose up login` → noVNC :6901 `401` ✓, CDP :9223 Chrome JSON ✓.
- **Done when:** ✅ **Prep complete.** App containerization is M8.

---

### M2 — Scrape spike (standalone, before any Rust)
- [x] `pip install "scrapling[all]" && scrapling install` — fetchers, `extract` CLI, browsers (bare `pip install scrapling` lacks fetchers → `ModuleNotFoundError`)
- [x] `docker compose up -d login` — login container boots (root compose); `curl http://localhost:9223/json/version` returns JSON (CDP is the harvest channel, **not** the scrape path)
- [x] Log into `id.jobstreet.com` once via the noVNC UI (http://localhost:6901) — note: jobstreet.co.id redirects here; cookies land on .jobstreet.com
- [x] `python session.py` — harvests cookies to `session.json` (§2.4); printed 20 cookies (SESSION_HOST fixed to "jobstreet.com" — see Architecture §4)
- [x] Probe before wiring selectors: confirmed `[data-automation="job-detail-title"]` and `[data-automation="jobAdDetails"]` work; spec selectors (jobDescriptionText/jobDescription) do not exist on live pages
- [x] `scrape.py` from §4 (own browser + harvested cookies), runnable on its own with the login container **stopped** — proves the scrape path is decoupled
- [x] Run against **3 real** JobStreet job-detail URLs (different categories: tech/management, hospitality, logistics)
- [x] Inspect `{"title","description"}` JSON on stdout — both fields non-empty for all 3
- [x] Selectors tuned: swapped jobDescriptionText→jobAdDetails; fixed scrapling API (css() not css_first()); **Architecture §4 updated** to match what works.
- **Verify:** `python scrape.py <url>` prints valid JSON three times in a row **with the kasmvnc container down**. ✅ Done.
- **Done when:** 3/3 sample URLs return non-empty title + description, container-independent. No Rust touched yet. ✅
- **Why first:** site HTML + the harvested session are the only pieces we don't control. Don't wire a scraper you haven't seen print JSON.

### M3 — Full backend in one pass (DB + templates + pipeline + mock LLM)
*v1 plan's M3+M4 merged — `cargo check` alone proves nothing; the only real verification is the end-to-end happy path.*

- [x] `src/db.rs`: every function referenced in §5 (`create_job_stub`, `set_status`, `get_job_url`, `update_job_data`, `get_status`, `save_cv_draft`, `get_master_cv`, `render_cv_ready`, settings get/upsert). Plain `sqlx::query` — no compile-time macro dance unless a typo bites.
- [x] `src/templates.rs` + `templates/`: askama structs from §5.3; HTML from §6 (base, index, job, settings, fragments/processing, fragments/cv_ready). Dashboard input includes the `pattern="…id.jobstreet.com/…"` guard from §6.2.
- [x] `src/generate.rs`: `process_job`, `fetch_job`, `scrape_once`, `build_prompt`, `call_llm` (mock branch only).
- [x] Routes in `src/main.rs`: `POST /jobs` (with `is_phase1_url` host guard), `GET /jobs/:id/card`, `GET /`.
- [x] HTMX polling card lifecycle: processing → cv_ready (stops polling).
- **Verify:** with `LLM_MOCK=true make dev`, paste a real JobStreet URL → polling card appears → resolves to "CV ready" within ~15s. Inspect DB row: status ends at `pending_approval`, `cv` column is non-empty.
- **Done when:** full happy path works offline against mock LLM. Templates, DB, and pipeline all proven at once. ✅

### M4 — Real LLM call
- [x] Implement non-mock branch of `call_llm` against `LLM_ENDPOINT`
- [x] Confirm JSON shape matches the §5.6 schema (summary / skills / experiences)
- **Verify:** unset `LLM_MOCK`, repeat M3's flow → CV content is clearly derived from the job description, not the mock.
- **Done when:** same flow as M3 produces a real tailored CV.

### M5 — Review + decision (`GET /jobs/:id`, `POST /jobs/:id/decision`)
- [x] Two-panel CV review page from §6.4
- [x] Approve → status `approved`. Reject → status `rejected`, reason stored in `reject_reason` (column added in M1, no new migration).
- **Verify:** approve and reject both work; reason persists across page reload.
- **Done when:** decision flow is fully clickable and persists.

### M6 — Settings (master CV editor)
- [x] `GET /settings`, `POST /settings` (upsert on single-row table)
- [x] Confirm `process_job` reads the saved master CV into the prompt
- **Verify:** save master CV → run another job → generated CV references skills/experiences from the master.
- **Done when:** master CV feeds generation.

### M7 — Hardening pass (only what's actually bitten)
- [x] Scrape failure → status `failed` → card shows error.
- [x] Duplicate URL → return the existing row's card (feature, not error: a UNIQUE collision means the user already pasted this URL — surface it, don't 500). Update Architecture §5.7 when this lands.
- [x] One `assert`-based self-check per non-trivial module per the ponytail rule — `generate.rs` and `db.rs` get a `demo()` or `#[test]`. Nothing else unless observed.
- [x] Anything else only if observed during M1–M6 — no speculative error handling.
- **Verify:** 10-URL test session produces no unhandled panics in `cargo run`.
- **Done when:** Phase 1 exit criteria from Architecture §1.1 met — 10 sample JobStreet URLs flow through the full UI without manual intervention.

---

### M8 — Containerize the app + CD (after M7)

- [x] Multi-stage `Dockerfile` at root: build the Rust binary, then a runtime layer with Python + `scrapling[all]` + Chromium (`playwright install chromium`)
- [x] Add the `app` service to the root `docker-compose.yml` (build: ., :3000, `CDP_URL=http://login:9223`, `app_data` volume for SQLite + `session.json`, `depends_on: [login]`)
- [x] On a clean VM/LXC with only Docker: `docker compose up` → paste a JobStreet URL at :3000 → CV generates; session harvested via :6901
- [x] CD: on push to `main`, the self-hosted runner rebuilds + restarts the stack on the target VM (`.github/workflows/cd.yml`)
- **Verify:** the full Phase-1 flow runs from `docker compose up` alone on a machine that has nothing but Docker.
- **Done when:** deployable to any Linux VM/LXC with `git clone && docker compose up`.

---

### M9 — Fix experiences prompt fidelity (observed defect)

The 2026-06-21 end-to-end verify returned an **empty `experiences[]`** — summary + skills were job-derived, but DeepSeek skipped experiences entirely. The generated CV is incomplete.

- [x] Tighten `build_prompt` in `src/generate.rs` — explicitly require ≥1 experience, name every field, show the exact JSON shape. Consider a stricter system instruction or a one-shot example.
- [x] Re-run a real `id.jobstreet.com` URL → confirm `experiences` is non-empty and derived from the JD
- [x] **Bonus fix (uncovered during verify):** bumped `max_tokens` 2048 → 4096. The stricter M9 prompt makes DeepSeek emit longer, more thorough CVs that overran 2048 and arrived truncated mid-JSON (`EOF while parsing`). Documented in `deepseek-prompt-fidelity` skill.
- **Verify:** `experiences` array has ≥1 entry (company + role + bullet_points) for 3/3 sample URLs. ✅ Verified 2026-06-22 on URLs `87400340`, `92841552`, `92795955` — each returned 3 experiences, all JD-tailored.
- **Done when:** a generated CV includes experiences, not just summary + skills. ✅

### M10 — Verify containerized deploy (M8 unverified)

M8 wrote the Dockerfile + compose but the full stack was never run in containers — only built.

- [x] `docker compose up --build` — both `login` + `app` start cleanly
- [x] Get `session.json` into the `app_data` volume (copy from host, or `docker compose exec app python session.py` after logging in via `:6901`)
- [x] Paste a JobStreet URL at `:3000` → CV generates inside the container
- [x] Fix anything that only works on bare metal (paths, sandbox, env)
- **Verify:** full Phase-1 flow works from `docker compose up` on this host. ✅ Verified 2026-06-22 — fresh anonymous volume: app bootstrapped its own db + served; POSTed `id.jobstreet.com/job/87400340` → real LLM CV (13 JD-derived skills, 1 experience w/ 4 quantified bullets), clean logs.
- **Done when:** containerized end-to-end is **proven**, not just built. ✅
- **What broke (all container-only, now in `docker-multistage` skill):**
  1. **glibc mismatch** — builder `rust:*-slim` (Debian trixie, 2.41) over runtime `ubuntu:22.04` (2.35): binary failed `GLIBC_2.39 not found`. Fixed by matching builder+runtime to the same distro (both `debian:bookworm`, 2.36).
  2. **`create_if_missing` defaults false** — `SqlitePool::connect()` can't *create* the db on a fresh volume → `SQLITE_CANTOPEN`. Bare metal always worked only because `jobagent.db` pre-existed from M1. Fixed via `SqliteConnectOptions::create_if_missing(true)`. (A musl-static detour was a red herring — it broke SQLite file access entirely.)
  3. **stale container name + host port** — the Prep-era `jobhunting-login` smoke-test container and a leftover bare-metal `cargo run` on `:3000` blocked `compose up`. Removed both before the clean bring-up.

### M11 — Verify CD pipeline

`.github/workflows/cd.yml` was written in M8 but never triggered.

- [ ] Push a trivial change to `main` → confirm the CD run executes on the self-hosted runner
- [ ] Confirm it rebuilds + restarts the stack (`docker compose up -d --build`)
- [ ] Confirm zero manual steps end-to-end
- **Verify:** a push to `main` results in a redeployed stack.
- **Done when:** CD is **proven**, not just written.

---

## Explicitly deferred

- **Other 42 sites** → Phase 2, one at a time, ordered by xlsx category
- **Per-site selector config map** → Phase 2, when the second site is attempted; Scrapling's `adaptive=True`/`auto_save` (self-healing selectors) may reduce or replace it
- **Listing/index crawler** → Phase 3, only if per-URL proves insufficient; Scrapling's `Spider` framework (concurrency, pause/resume, multi-session) is the vehicle
- **Separate scrape container** → alternative to M8's fat app image; base the scraper on `pyd4vinci/scrapling` if the app image gets too heavy. M8 bakes the scraper into the app image by default
- **Per-function test suites / framework** → only if M7's self-checks prove insufficient
- **Auth, multi-user** → never; single-user. **Deployment** → M8: containerized, `docker compose up` on any Linux VM/LXC; CI on self-hosted GitHub Actions (Prep)
