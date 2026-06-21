# Multi-stage Dockerfile for the JobHunting app service (Architecture §9.1, M8).
#
# Stage 1 (builder): compile the Rust binary (with sqlx migrations embedded).
# Stage 2 (runtime): Ubuntu 22.04 with Python + scrapling[all] + Chromium.
#   The binary and Python scripts are copied in; both run from /app.
#
# Build context: repo root (docker build .)

# ── Stage 1: Rust builder ────────────────────────────────────────────────────
# Note: rust:1.82-slim is too old — current transitive deps (idna_adapter,
# home) require the edition2024 cargo feature stabilized in rustc 1.85.
# 1.95 gives headroom; bump intentionally when upgrading.
FROM rust:1.95-slim AS builder

WORKDIR /build

# Install build dependencies (OpenSSL / pkg-config needed by reqwest)
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# NOTE: sqlx-cli is NOT installed here. Migrations are embedded into the Rust
# binary at build time via `sqlx::migrate!("./migrations")` in src/main.rs and
# run on startup. This avoids the recurring build breakage from
# `cargo install sqlx-cli` pulling a version that needs a newer rustc than the
# image ships (M10 gotcha — see docker-multistage skill).

# Cache dependencies: copy manifests first, build deps only, then copy source.
COPY Cargo.toml Cargo.lock ./

# Create a dummy main so `cargo build --release` caches all deps before we copy
# the real source. This layer is rebuilt only when Cargo.toml/Cargo.lock change.
RUN mkdir src && echo 'fn main() {}' > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Now copy real source + templates/migrations and do the final build.
COPY src/ src/
COPY templates/ templates/
COPY migrations/ migrations/

# Touch main.rs so cargo knows it changed (dummy build above left stale metadata).
RUN touch src/main.rs && cargo build --release

# ── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM ubuntu:22.04 AS runtime

# Avoid interactive prompts from apt/tzdata during build.
ENV DEBIAN_FRONTEND=noninteractive

# System packages: Python 3, pip, and the libs Playwright Chromium needs.
# We install Python first, then scrapling[all] which pulls in playwright, then
# `playwright install chromium` which downloads Playwright's Chromium binary.
# Playwright Chromium runtime deps: libnss3 through fonts-liberation.
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        python3 \
        python3-pip \
        python3-venv \
        ca-certificates \
        curl \
        libnss3 \
        libnspr4 \
        libdbus-1-3 \
        libatk1.0-0 \
        libatk-bridge2.0-0 \
        libcups2 \
        libdrm2 \
        libxkbcommon0 \
        libxcomposite1 \
        libxdamage1 \
        libxfixes3 \
        libxrandr2 \
        libgbm1 \
        libasound2 \
        libpango-1.0-0 \
        libcairo2 \
        libxshmfence1 \
        fonts-liberation \
    && rm -rf /var/lib/apt/lists/*

# Install scrapling with all extras; this pulls in playwright.
# Pin to avoid surprise breakage — bump intentionally when upgrading.
# 0.4.9 is the version verified working for this project (see scrapling-jobstreet skill).
RUN pip3 install --no-cache-dir "scrapling[all]==0.4.9"

# Install Playwright's Chromium browser binary (the one scrapling's DynamicFetcher uses).
# We use `playwright install chromium` directly instead of `scrapling install` because:
# - system deps are already installed above via apt
# - `scrapling install` calls `playwright install-deps` which re-runs apt (redundant)
# - `playwright install chromium` is the direct equivalent and more explicit
RUN python3 -m playwright install chromium

# Create the app working directory.
WORKDIR /app

# Copy the compiled binary from the builder stage.
# Migrations are already embedded inside it via sqlx::migrate!() — no sqlx-cli
# binary needed at runtime. Askama templates are also compiled in at build time
# via #[template(path = "...")], so neither templates/ nor migrations/ need to
# be copied to the runtime image.
COPY --from=builder /build/target/release/job-agent /app/job-agent

# Copy Python scripts the binary shells out to at runtime (scrape.py).
COPY scrape.py session.py ./

# Expose the web UI port.
EXPOSE 3000

# The binary runs embedded migrations on startup, so CMD is just the server.
# DATABASE_URL and LLM_* are supplied via compose environment.
# SESSION_FILE defaults to /data/session.json (overridden via compose env).
ENV SESSION_FILE=/data/session.json
CMD ["/app/job-agent"]
