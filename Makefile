.PHONY: dev migrate

# ponytail: .env lives at project root (shared across worktrees); each worktree's
# Makefile sources it via ../.env. See Architecture §8.
migrate:
	set -a; . ../.env; set +a; \
	sqlx database create; \
	sqlx migrate run

dev: migrate
	set -a; . ../.env; set +a; \
	exec cargo watch -x run
