---
name: docker-multistage
description: Gotchas building a multi-stage Rust + Python/Scrapling/Playwright Docker image
---

# Docker Multi-Stage: Rust + Python + Scrapling

## Gotcha 1: Comments inside RUN apt-get lists break the build

**Problem:** Putting a `# comment` inside a `RUN apt-get install` package list causes apt to try to install a package named `#`, which fails immediately.

**Root cause:** Shell heredoc-style comments are not supported inline in a single `RUN` command's continuation lines — the `#` is passed literally to apt as a package name.

**Fix:** Put the comment on the line *before* the `RUN`, not inside it:
```dockerfile
# Playwright Chromium runtime deps follow
RUN apt-get install -y \
    libnss3 \
    libgbm1 \
    ...
```

---

## Gotcha 2: scrapling install vs playwright install chromium

**Problem:** `scrapling install` calls both `playwright install chromium` AND `playwright install-deps chromium`. Inside a Docker build where system deps are already installed via apt, `playwright install-deps` re-runs apt-get (slow, redundant, sometimes fails if sources are stale).

**Root cause:** `scrapling install` is designed for developer machines where nothing is pre-installed.

**Fix:** Use `python3 -m playwright install chromium` directly in the Dockerfile. The system deps are already installed via the earlier `apt-get install` layer. This is faster and avoids the redundant apt pass.

---

## Gotcha 3: Server binding to 127.0.0.1 is invisible outside the container

**Problem:** If the Rust server binds to `127.0.0.1:3000`, `docker compose up` starts fine but `curl http://localhost:3000` from the host returns connection refused.

**Root cause:** `127.0.0.1` is the container's loopback — only processes inside the container can reach it. Docker's port publishing (`ports: ["3000:3000"]`) forwards from the host to the container's network interface, not to loopback.

**Fix:** Bind to `0.0.0.0:3000` or make the address configurable via `BIND_ADDR` env (default `0.0.0.0:3000`). For local dev where you want only-localhost exposure, override with `BIND_ADDR=127.0.0.1:3000`.

---

## Gotcha 4: sqlx migrate run needs sqlx-cli in the runtime image

**Problem:** Running `sqlx migrate run` in the container CMD requires the `sqlx` binary, but `ubuntu:22.04` doesn't have it.

**Root cause:** `sqlx-cli` is a Rust CLI tool not available via apt.

**Fix:** Build `sqlx-cli` in the Rust builder stage and copy the binary into the runtime image:
```dockerfile
# Builder
RUN cargo install sqlx-cli --no-default-features --features sqlite
# Runtime
COPY --from=builder /usr/local/cargo/bin/sqlx /usr/local/bin/sqlx
```

**ponytail:** ✅ Adopted in M10 — the project now embeds migrations via `sqlx::migrate!("./migrations")` in the Rust binary (runs on startup). sqlx-cli was removed from both stages. This also dodges the recurring build breakage where `cargo install sqlx-cli` pulls a crate needing a newer rustc than the image ships.

---

## Gotcha 5: .dockerignore is critical for Rust projects

**Problem:** Without `.dockerignore`, Docker sends the entire build context including `target/` to the daemon. `target/` for a Rust project can be multiple GB — this makes every `docker build` extremely slow even on unchanged code.

**Fix:** Always create `.dockerignore` at the repo root with at minimum:
```
target/
```

---

## Gotcha 6: glibc mismatch between builder and runtime stages

**Problem:** The container starts, then the binary fails immediately:
```
/app/job-agent: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found
```

**Root cause:** The builder and runtime stages use *different distros*, so different glibc versions. The binary is linked against the builder's glibc and won't load on an older runtime glibc. Concretely (M10): `rust:1.95-slim` is Debian trixie (glibc **2.41**); `ubuntu:22.04` is glibc **2.35**. The compiled binary needed symbols from 2.39+.

**Fix:** Match builder and runtime to the **same distro/version** so glibc can never drift. The boring choice that works: `rust:1.95-slim-bookworm` builder + `debian:bookworm` runtime (both glibc 2.36).
- Don't "fix" this by going musl-static (alpine builder) unless you've verified every native dep — see Gotcha 7. A glibc-matched pair is simpler and is what bare-metal dev already runs.
- Watch the pip side-effect of a newer Debian: bookworm/trixie enforce PEP 668, so `pip3 install` needs `--break-system-packages` (fine for a single-user container). The `t64` package renames (`libasound2`→`libasound2t64`, `libcups2`→`libcups2t64`) only bite trixie, not bookworm.

---

## Gotcha 7: `create_if_missing` defaults false — fresh-volume deploy panics SQLITE_CANTOPEN

**Problem:** `docker compose up` on a fresh volume (fresh VM, empty `/data`):
```
thread 'main' panicked at src/main.rs: failed to connect to SQLite:
Database(SqliteError { code: 14, message: "unable to open database file" })
```
But the same binary works fine on bare metal, and works in-container once `/data/jobagent.db` already exists.

**Root cause:** `SqlitePool::connect(url)` uses default `SqliteConnectOptions`, where **`create_if_missing` is false**. So sqlx can *open* an existing db but cannot *create* one. Bare metal never hit this because `jobagent.db` was already there from earlier setup — the app had never once bootstrapped its own db. A truly fresh deploy (the whole point of M8/M10) panics on first connect, before migrations can even run.

**Fix:** Parse the URL into options and enable create explicitly:
```rust
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
let pool = SqlitePool::connect_with(
    database_url.as_str()
        .parse::<SqliteConnectOptions>()
        .expect("bad DATABASE_URL")
        .create_if_missing(true),
).await.expect("failed to connect to SQLite");
```
(`SqliteConnectOptions` has no `From<&str>` in sqlx 0.7 — use `FromStr` via `.parse::<>()`, not `::from_url` or `::from`.)

**Red herring to skip:** building a **musl-static binary** (alpine builder) to "make glibc a non-issue" (see Gotcha 6) instead breaks bundled-SQLite file access entirely — every file-DB URL panics CANTOPEN; only `sqlite::memory:` works. Don't go down this path for a sqlx-sqlite app. Match the glibc pair (Gotcha 6) and fix the real default (this gotcha) instead.

---

## Gotcha 8: stale container names + host ports block `compose up`

**Problem:** `docker compose up` fails at container creation with either:
```
Conflict. The container name "/jobhunting-login" is already in use ...
```
or
```
failed to bind host port 0.0.0.0:3000/tcp: address already in use
```

**Root cause:** A previous smoke test / bare-metal run left a stopped container holding the fixed `container_name`, or a bare-metal `cargo run` is still bound to the host port compose wants. Compose's fixed `container_name` collides across compose projects too (e.g. a `main/` smoke test vs an `m10-docker/` worktree run).

**Fix:** Before the clean bring-up:
- `docker rm <name>` the stale stopped container (compose will recreate it).
- `kill` the leftover host process holding the port (`lsof -i :3000` / `ss -tlnp | grep :3000`).
Consider whether `container_name:` is even needed — without it, compose names per-project and won't collide.
