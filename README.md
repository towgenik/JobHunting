# JobHunting

Automated job search and CV tailoring platform. Scrapes job listings from
JobStreet, builds a tailored CV for each job description via LLM, and manages
the whole pipeline from a web UI.

Built as a personal tool: single-user, no accounts, self-hosted.

## Stack

- **Backend**: Rust + axum + Tokio + sqlx (SQLite)
- **Frontend**: server-rendered HTML, HTMX, zero JavaScript framework
- **LLM**: OpenRouter/Anthropic/OpenAI-compatible/Google endpoints, streaming SSE
- **Scraping**: Playwright (Python) with session cookie management
- **Deploy**: Docker Compose, multi-stage Rust build (musl static binary)

## Architecture

Profile-driven:

1. `profile/index.md` — markdown knowledge base (your CV as structured data,
   YAML frontmatter + wikilinks). This is the source of truth.
2. The Rust app syncs the profile into SQLite on startup and via `POST /profile/sync`.
3. The scraper reads job listings from JobStreet.
4. The LLM tailors the profile against each job description and generates a CV.

## Quickstart

```sh
cp .env.example .env   # fill in LLM_API_KEY etc.
docker compose up -d --build
# web UI:   http://localhost:3000
# noVNC:    http://localhost:6901  (login to job boards here; session.py harvests cookies over CDP)
```

For local dev (no containers):

```sh
cargo run
python session.py     # harvest logged-in cookies from the login container over CDP
```

## Layout

```
src/            Rust backend (axum handlers, pipeline, LLM transport, scraper)
templates/      server-rendered HTML (Askama)
migrations/     SQLite schema (sqlx)
profile/        career knowledge base — replace with your own data
login/          KasmVNC Chrome container: human login → cookie harvest
docs/           research notes, API reference, architecture docs
```

## Configuration

See `.env.example` for all variables. Key ones:

| Variable | Purpose |
|---|---|
| `LLM_ENDPOINT` / `LLM_API_KEY` / `LLM_MODEL` | LLM provider |
| `LLM_PROVIDER` | override auto-detection (`openai` / `anthropic` / `google`) |
| `CDP_URL` | Chrome DevTools Protocol endpoint for cookie harvesting |
| `SESSION_HOST` | cookie domain(s) to harvest |
| `PROFILE_DIR` | where `profile/index.md` lives (default `./profile`) |

## Development

```sh
cargo check --all-targets
cargo test
```

CI runs these on self-hosted runners; it also enforces AGENTS.md size and
that no secrets/DBs/sessions are tracked.

## Notes

- `docs/` contains third-party API research. Reverse-engineering material and
  traffic captures are intentionally not committed (see `.gitignore`).
- `profile/` ships with a placeholder — this tool is useless without your own
  career data, which is by design: it's your knowledge base, not a shared one.
