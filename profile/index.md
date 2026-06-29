---
name: "Farrel Ilham Shaputra"
title: "Software Engineer — Backend & Infrastructure"
updated: 2026-06-23
skills: [Rust, Python, Docker, Linux, SQL, Axum, Tokio, HTMX, Playwright, Tailscale, Caddy, Prometheus, Grafana, PostgreSQL, SQLite, REST, GraphQL, gRPC, Git, GitHub Actions, Bash, Systemd, Nginx]
target_roles: [Backend Engineer, Platform Engineer, DevOps Engineer, Site Reliability Engineer]
---

# Summary

Software engineer focused on backend systems and infrastructure automation.
I build from container to cloud — Rust APIs, CI/CD pipelines, and self-hosted
services. I treat infrastructure as product: monitored, backed up, documented.
Comfortable working solo or on small teams; I own features end-to-end.

# Tools & Technologies

**Languages**: Rust (primary, 3yr), Python (5yr), JavaScript/TypeScript (basic),
SQL (PostgreSQL, SQLite), Bash, HTML/CSS.

**Frameworks & Libraries**: Axum, Tokio, sqlx, HTMX, Askama, Serde, Reqwest,
Playwright, Scrapling, Pydantic.

**Infrastructure**: Docker (multi-stage builds, compose), Linux (daily driver,
Arch/CachyOS, systemd, shell scripting), Tailscale (mesh VPN), Caddy (reverse
proxy, auto-HTTPS), Nginx (basic).

**Observability**: Prometheus (metric collection, alert rules), Grafana
(dashboards), structured JSON logging, health-check endpoints.

**Data**: PostgreSQL (schema design, query optimization, indexing), SQLite
(embedded, FTS), Redis (caching, basic).

**CI/CD**: Docker Compose (single-VM deploy), GitHub Actions (lint + test +
build), health-check-based rolling restarts, Blue-Green (basic).

**AI/LLM**: OpenRouter API (DeepSeek, Claude, OpenAI-compatible), streaming
SSE responses, prompt engineering (DeepSeek fidelity patterns), RAG pipelines
(basic), local models via llama.cpp.

**Other**: Git (advanced), REST API design, GraphQL (basic), gRPC (basic),
WebSocket, JWT/OAuth2 auth, Playwright browser automation, web scraping
(anti-detection, session management).

# Skills

- **Backend Engineering**: Designed and deployed production Rust services with
  Axum + Tokio. Comfortable with async runtime internals, connection pooling,
  middleware chains, error handling patterns, and API versioning.
- **Infrastructure & DevOps**: Self-hosted infrastructure with Docker Compose,
  Tailscale mesh networking, and Caddy reverse proxy. Automated deployments
  with health-check-based rolling restarts. Prometheus + Grafana monitoring.
- **Web Scraping & Automation**: Built production scrapers with Playwright +
  Python (Scrapling). Handles anti-bot detection, session cookie management,
  proxy rotation, and structured data extraction from dynamic SPAs.
- **LLM Integration**: Integrated streaming LLM APIs (OpenRouter, Anthropic)
  with SSE for real-time responses. Built prompt templates with structured
  JSON output enforcement. Experience with local models via llama.cpp.
- **Full-Stack Development**: Built complete applications with Rust backends
  and HTMX frontends. Server-rendered HTML with progressive enhancement.
  SQLite + sqlx for data, Askama for templating.

# Experience

## Freelance Backend Developer (2023 – Present)

**JobHunting** — Automated job search and CV tailoring platform (this project).
- Built crawling pipeline that scrapes 200+ listings/day from JobStreet using
  Playwright + Python with session cookie management and anti-detection.
- Designed Rust/Axum backend with HTMX frontend — zero JavaScript framework,
  server-rendered HTML with progressive enhancement via hx-* attributes.
- Integrated OpenRouter API (DeepSeek V4 Flash, Claude) with streaming SSE
  responses — token-by-token rendering with reasoning visibility.
- Implemented SQLite + sqlx data layer with UUID keys, migration-based schema
  evolution, and FTS for search.
- Docker Compose deployment with multi-stage Rust builds (musl static binary),
  health-check-based restarts, and env_file-based configuration.
- Built APCA/OKLCH theme engine that reads OS system colors for automatic
  light/dark mode — 200 lines of vanilla JS, no library.
- See [[portfolio#jobhunting|JobHunting portfolio]].

**API Gateway** — High-throughput Rust proxy for microservice routing.
- Handled 10k+ concurrent connections at <5ms p99 latency with Tokio async I/O.
- Implemented JWT authentication middleware with role-based access control.
- PostgreSQL connection pooling via sqlx with prepared statement caching.
- Structured JSON logging with trace IDs for distributed request tracking.
- Prometheus metrics endpoint: request counts, latency histograms, error rates.
- See [[portfolio#api-gateway|API Gateway portfolio]].

## Personal Projects

**[[portfolio#homelab|Homelab]]** — Self-hosted infrastructure on a single machine.
- 14 Docker containers across multiple compose projects with Tailscale mesh VPN
  for secure remote access.
- Caddy reverse proxy with automatic Let's Encrypt HTTPS for all services.
- Prometheus + Grafana monitoring stack with alert rules for disk, memory, CPU.
- Daily automated restic backups to Backblaze B2 with retention policies.
- Services: Gitea, Prometheus, Grafana, Caddy, Tailscale, plus project apps.

**Scripting & Automation** — Various Python/Rust utilities for personal workflow.
- Automated media organization pipeline with metadata extraction and transcoding.
- System maintenance scripts: log rotation, disk cleanup, health checks.
- Data migration tools: CSV/JSON transformation, API data sync, ETL pipelines.

# Work Preferences

- **Location**: Remote preferred. Based in Indonesia (WIB timezone). Open to
  hybrid or on-site in Jakarta/Bandung for the right role.
- **Industries**: DevOps tooling, cloud infrastructure, developer tools, AI/ML
  platforms, open-source companies. Less interested in: ad-tech, crypto/blockchain,
  gambling, defence.
- **Team size**: Comfortable on teams of 3–15. Happy as solo backend owner or
  collaborating across functions. Prefer async communication (written over meetings).
- **Tech preferences**: Rust-first roles ideal, but open to Go, Python, or
  polyglot stacks. Prefer Linux-native development environments. Strongly prefer
  companies that invest in observability and CI/CD.
- **Deal-breakers**: Micromanagement, mandatory open-plan offices for remote
  roles, on-call without compensation or rotation limits.

# Soft Skills

- **Communication**: Write clear technical documentation and RFC-style proposals.
  Prefer async written communication over synchronous meetings. Comfortable
  presenting architecture decisions to technical audiences.
- **Work style**: Self-directed, comfortable with ambiguity. I break down large
  problems into shippable milestones. I ask for help early rather than spinning
  wheels.
- **Code review**: Constructive, focused on design and maintainability rather
  than style nits. I review code the way I want mine reviewed.
- **Learning**: Continuous learner through building. I learn new tools by
  shipping real projects, not tutorials. Currently deepening Kubernetes and AWS.

# Languages

- **Indonesian**: Native
- **English**: Professional working proficiency (reading, writing, speaking)
- **Sundanese**: Conversational

# Education & Certifications

- **Self-taught** — Continuous learning through building projects, reading
  documentation, and contributing to technical communities.
- AWS Solutions Architect Associate (SAA-C03) — in progress, expected Q3 2026
- Certified Kubernetes Administrator (CKA) — planned Q4 2026
- Various Coursera/edX courses in distributed systems, databases, and networking
