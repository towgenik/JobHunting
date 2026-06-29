# AGENTS.md

**JobHunting** — single-user tool: scrape JobStreet listings → generate tailored CVs via LLM. Rust + axum + HTMX + SQLite. Career knowledge wiki in `profile/`.

## Profile-driven architecture

The master CV lives in `profile/index.md` (markdown + YAML frontmatter) — a knowledge base, not a resume. Claude Code edits it via `/cv:build`, `/cv:edit`, `/cv:suggest`. The Rust app syncs it to SQLite on startup and via `POST /profile/sync`. The scraper reads it from DB and tailors against each job description.

See `.claude/skills/cv/SKILL.md` for the profile management skill.

## Hard rules

- **Never run a second app instance** — this machine has limited resources. Kill the running container before starting a new one. Only one `job-agent` process on the host.
- **Sudo is allowed** — password is `user`. Use `echo "user" | sudo -S ...` for automated commands.
- **Never edit `.env`** — user's config.
- **Never commit secrets or build trash.** `.gitignore` covers `/target`, `*.db*`, `.env*`, `*.pem`, `*.key`, `*.p12`, `secrets/`, `.venv/`.
