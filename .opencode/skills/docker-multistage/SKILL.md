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

**ponytail:** An alternative is to embed migrations via `sqlx::migrate!()` macro in the Rust binary itself, which runs them on startup. That eliminates the sqlx-cli dependency entirely. Switch to that if the sqlx-cli binary causes issues (glibc version mismatch between builder and runtime).

---

## Gotcha 5: .dockerignore is critical for Rust projects

**Problem:** Without `.dockerignore`, Docker sends the entire build context including `target/` to the daemon. `target/` for a Rust project can be multiple GB — this makes every `docker build` extremely slow even on unchanged code.

**Fix:** Always create `.dockerignore` at the repo root with at minimum:
```
target/
```
