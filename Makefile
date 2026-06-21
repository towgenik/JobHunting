.PHONY: dev migrate

migrate:
	set -a; . ./.env; set +a; \
	sqlx database create; \
	sqlx migrate run

dev: migrate
	set -a; . ./.env; set +a; \
	exec cargo watch -x run
