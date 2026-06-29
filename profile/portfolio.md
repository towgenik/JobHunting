# Portfolio

## api-gateway {#api-gateway}

![[portfolio/api-gateway.svg]]

High-throughput API gateway built with Rust + Tokio + Axum.
- **Scale**: 10k+ concurrent connections at <5ms p99 latency
- **Features**: JWT auth middleware, role-based access control, rate limiting
- **Stack**: Rust, Tokio, Axum, sqlx, PostgreSQL
- **Monitoring**: Prometheus metrics endpoint, structured JSON logging

[[index#skills|← Back to skills]] | [[index#experience|← Back to experience]]

## jobhunting {#jobhunting}

![[portfolio/jobhunting.svg]]

Automated job search and CV tailoring pipeline — this project.
- **Pipeline**: scrape → generate → review → approve
- **Scale**: 200+ listings/day from JobStreet
- **Stack**: Rust + Axum + HTMX + SQLite + Docker
- **AI**: OpenRouter API (DeepSeek, Claude) with streaming SSE responses

[[index#experience|← Back to experience]]

## homelab {#homelab}

![[portfolio/homelab.svg]]

Self-hosted infrastructure on a single machine.
- **Containers**: 14 services via Docker Compose
- **Network**: Tailscale mesh VPN, Caddy reverse proxy with auto-HTTPS
- **Monitoring**: Prometheus + Grafana with alert rules
- **Backups**: Daily restic snapshots to Backblaze B2

[[index#experience|← Back to experience]]
