.PHONY: dev

# ponytail: migrations are embedded in the binary (sqlx::migrate!()), so they run
# automatically on startup. No sqlx CLI needed. .env lives at project root.

dev:
	set -a; . ../.env; set +a; \
	exec cargo run
