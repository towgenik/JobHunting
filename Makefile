.PHONY: dev migrate

# ponytail: .env lives at project root (shared across worktrees); each worktree's
# Makefile sources it via ../.env. See Architecture §8.
# ponytail: ensure ~/.cargo/bin on PATH so sqlx-cli is found even in non-login shells.
migrate:
	set -a; . ../.env; set +a; \
	export PATH="$$HOME/.cargo/bin:$$PATH"; \
	sqlx database create; \
	sqlx migrate run

dev: migrate
	set -a; . ../.env; set +a; \
	export PATH="$$HOME/.cargo/bin:$$PATH"; \
	exec cargo watch -x run
