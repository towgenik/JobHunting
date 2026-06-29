# API Skill — AI Agent Debug & Control Interface

JSON API under `/api/*` for driving and debugging the running app via `curl`.

## When to use

- After recompiling, verify the app works end-to-end.
- Submit jobs, poll status, inspect CVs, regenerate with feedback — all without the HTMX UI.
- Inspect DB state for debugging (arbitrary SELECT queries).

## Prerequisites

- Server running: `cargo run` (or `LLM_MOCK=true cargo run` for offline)
- `curl` and `jq` available

## Boot check

```bash
curl -sf localhost:3000/api/health | jq .
```

Returns `{"ok": true, "version": "0.1.0", "scheduler_running": false, "crawl_active": false}`.

## Core workflow

```bash
# 1. Submit a job
JOB_ID=$(curl -sf -X POST localhost:3000/api/jobs \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://id.jobstreet.com/job/12345"}' | jq -r .id)

# 2. Poll status (repeat until terminal: 'generated' or 'failed')
curl -sf "localhost:3000/api/jobs/$JOB_ID/card" | jq .

# 3. Get full detail (CV, review, rank)
curl -sf "localhost:3000/api/jobs/$JOB_ID" | jq .

# 4. Regenerate with feedback
curl -sf -X POST "localhost:3000/api/jobs/$JOB_ID/regenerate" \
  -H 'Content-Type: application/json' \
  -d '{"feedback":"Add more metrics to bullet points"}'
```

## All routes

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/api/health` | — | `{ok, version, scheduler_running, crawl_active}` |
| GET | `/api/jobs` | — | `{jobs: [{id, url, title, status}]}` |
| POST | `/api/jobs` | `{url}` | `{id, existing}` |
| GET | `/api/jobs/:id` | — | full record incl. cv, review, verification, rank |
| GET | `/api/jobs/:id/card` | — | `{id, status}` (lightweight polling) |
| POST | `/api/jobs/:id/regenerate` | `{feedback?}` | `{ok}` |
| DELETE | `/api/jobs/:id` | — | `{ok}` |
| POST | `/api/jobs/delete-batch` | `{ids: [...]}` | `{deleted: N}` |
| GET | `/api/profile?file=index.md` | — | `{files, current, content}` |
| POST | `/api/profile` | `{file, content}` | `{ok}` |
| POST | `/api/profile/sync` | — | `{ok}` |
| GET | `/api/settings/llm` | — | `{endpoint, model, openai_compat, mock_llm, api_key_suffix}` |
| POST | `/api/settings/llm` | `{endpoint, api_key, model, openai_compat?, mock_llm?}` | `{ok}` |
| GET | `/api/settings/scheduler` | — | `{interval_minutes, date_range, max_pages}` |
| POST | `/api/settings/scheduler` | `{interval_minutes, date_range, max_pages}` | `{ok}` |
| POST | `/api/scheduler/run` | — | `{accepted, run_id}` |
| GET | `/api/crawl/status` | — | `{active, stopping, message, terminal, total}` |
| POST | `/api/crawl/stop` | — | `{ok}` |
| GET | `/api/db/query?q=SELECT...` | — | `{columns, rows}` (SELECT/WITH only) |

## Debug tips

- **DB inspection**: `curl -sf 'localhost:3000/api/db/query?q=SELECT+status,+count(*)+FROM+jobs+GROUP+BY+status' | jq .`
- **Logs**: run server with `cargo run 2> /tmp/server.log`, then `tail /tmp/server.log`
- **Raw DB**: `sqlite3 jobagent.db "SELECT ..."` works from bash directly
- **Job not progressing?** Check `/api/jobs/:id/card` — status `failed` means check logs
- **LLM mock**: set `LLM_MOCK=true` to skip real API calls during testing

## Smoke test — core API

Copy-paste this entire block into bash from the project root (`main/`). It starts
a server with mock LLM on a temp DB, exercises all API endpoints, and cleans up.
Self-contained — no files to create, no dependencies beyond curl/jq/cargo/sqlite3.
Exits 0 on success, 1 on failure.

```bash
bash <<'SMOKE'
set -euo pipefail
DB="/tmp/jobagent_smoke_$$.db"
PORT=31999
BASE="http://localhost:$PORT"
MOCK_DIR=$(mktemp -d)
SERVER_PID=""
cleanup() { kill "$SERVER_PID" 2>/dev/null || true; rm -f "$DB"; rm -rf "$MOCK_DIR"; }
trap cleanup EXIT
fail() { echo "FAIL: $1" >&2; exit 1; }
pass() { echo "PASS: $1"; }

# Mock python — intercepts `python scrape.py` calls
cat > "$MOCK_DIR/python" << 'MOCKPY'
#!/bin/bash
[[ "${1:-}" == "scrape_api.py" ]] && echo '{"title":"Smoke Test Job","description":"A test job for API smoke testing."}' && exit 0
exec /usr/bin/python "$@"
MOCKPY
chmod +x "$MOCK_DIR/python"

# Run from project root (main/). The agent should cd there first.
set -a; . ../.env; set +a
export DATABASE_URL="sqlite://$DB"
export LLM_MOCK=true
export BIND_ADDR="127.0.0.1:$PORT"
export PATH="$MOCK_DIR:$PATH"

rm -f "$DB"
cargo run --quiet &
SERVER_PID=$!
for i in $(seq 1 30); do curl -sf "$BASE/api/health" >/dev/null 2>&1 && break; sleep 1; done
curl -sf "$BASE/api/health" | jq -e '.ok == true' >/dev/null || fail "health"
pass "health"

# Submit
RESULT=$(curl -sf -X POST "$BASE/api/jobs" -H 'Content-Type: application/json' \
  -d '{"url":"https://id.jobstreet.com/job/smoke-001"}')
JOB_ID=$(echo "$RESULT" | jq -r .id)
[ -n "$JOB_ID" ] && [ "$JOB_ID" != "null" ] || fail "submit: $RESULT"
pass "submit → $JOB_ID"

# Poll until terminal
for i in $(seq 1 30); do
  STATUS=$(curl -sf "$BASE/api/jobs/$JOB_ID/card" | jq -r '.status')
  case "$STATUS" in generated|failed) break ;; esac
  sleep 1
done
case "$STATUS" in generated|failed) ;; *) fail "poll: $STATUS" ;; esac
pass "poll → $STATUS"

# Detail
DETAIL=$(curl -sf "$BASE/api/jobs/$JOB_ID")
echo "$DETAIL" | jq -e '.id' >/dev/null || fail "detail: $DETAIL"
[ "$STATUS" = "generated" ] && echo "$DETAIL" | jq -e '.cv.summary' >/dev/null || true
echo "$DETAIL" | jq -e '.company' >/dev/null || fail "detail: missing company"
echo "$DETAIL" | jq -e '.url' >/dev/null || fail "detail: missing url"
pass "detail (company+url)"

# List
curl -sf "$BASE/api/jobs" | jq -e '.jobs | length >= 1' >/dev/null || fail "list"
curl -sf "$BASE/api/jobs" | jq -e '.jobs[0].company' >/dev/null || fail "list: missing company"
curl -sf "$BASE/api/jobs" | jq -e '.jobs[0].url' >/dev/null || fail "list: missing url"
pass "list (company+url)"

# Regenerate (only if generated)
OLD_SUMMARY=""
if [ "$STATUS" = "generated" ]; then
  OLD_SUMMARY=$(echo "$DETAIL" | jq -r '.cv.summary')
  curl -sf -X POST "$BASE/api/jobs/$JOB_ID/regenerate" \
    -H 'Content-Type: application/json' \
    -d '{"feedback":"Make the summary shorter"}' | jq -e '.ok == true' >/dev/null || fail "regenerate"
  pass "regenerate"
  # Poll again after regenerate
  STATUS="generating"
  for i in $(seq 1 30); do
    STATUS=$(curl -sf "$BASE/api/jobs/$JOB_ID/card" | jq -r '.status')
    case "$STATUS" in generated|failed) break ;; esac
    sleep 1
  done
  [ "$STATUS" = "generated" ] || fail "regenerate poll: $STATUS"
  pass "regenerate poll → $STATUS"
  # Detail after regenerate
  NEW_DETAIL=$(curl -sf "$BASE/api/jobs/$JOB_ID")
  NEW_SUMMARY=$(echo "$NEW_DETAIL" | jq -r '.cv.summary')
  echo "$NEW_DETAIL" | jq -e '.cv.summary' >/dev/null || fail "regenerate detail"
  echo "$NEW_DETAIL" | jq -e '.company' >/dev/null || fail "regenerate: company missing"
  pass "regenerate detail"
fi

# Delete
curl -sf -X DELETE "$BASE/api/jobs/$JOB_ID" | jq -e '.ok == true' >/dev/null || fail "delete"
pass "delete"

echo ""
echo "ALL PASSED"
SMOKE
```

## Smoke test — orphaned job recovery

Tests that jobs stuck at `status='new'` (created but never processed) are
recovered to `failed` on server restart. Run from project root (`main/`).

```bash
bash <<'RECOVERY'
set -euo pipefail
DB="/tmp/jobagent_recovery_$$.db"
PORT=31998
BASE="http://localhost:$PORT"
MOCK_DIR=$(mktemp -d)
SERVER_PID=""
cleanup() { kill "$SERVER_PID" 2>/dev/null || true; rm -f "$DB"; rm -rf "$MOCK_DIR"; }
trap cleanup EXIT
fail() { echo "FAIL: $1" >&2; exit 1; }
pass() { echo "PASS: $1"; }

cat > "$MOCK_DIR/python" << 'MOCKPY'
#!/bin/bash
[[ "${1:-}" == "scrape_api.py" ]] && echo '{"title":"Test","description":"Test"}' && exit 0
exec /usr/bin/python "$@"
MOCKPY
chmod +x "$MOCK_DIR/python"

# Run from project root (main/). The agent should cd there first.
set -a; . ../.env; set +a
export DATABASE_URL="sqlite://$DB"
export LLM_MOCK=true
export BIND_ADDR="127.0.0.1:$PORT"
export PATH="$MOCK_DIR:$PATH"

rm -f "$DB"
cargo run --quiet &
SERVER_PID=$!
for i in $(seq 1 30); do curl -sf "$BASE/api/health" >/dev/null 2>&1 && break; sleep 1; done
curl -sf "$BASE/api/health" | jq -e '.ok == true' >/dev/null || fail "health"

# Insert an orphaned 'new' job directly into DB
ORPH_ID="orphan-test-$(date +%s)"
sqlite3 "$DB" "INSERT INTO jobs (id, url, status, created_at) VALUES ('$ORPH_ID', 'https://id.jobstreet.com/job/orphan', 'new', datetime('now'))"
ORPH_STATUS=$(sqlite3 "$DB" "SELECT status FROM jobs WHERE id='$ORPH_ID'")
[ "$ORPH_STATUS" = "new" ] || fail "insert orphan: $ORPH_STATUS"
pass "inserted orphan (status=new)"

# Kill server (simulate crash)
kill -9 $SERVER_PID 2>/dev/null
wait $SERVER_PID 2>/dev/null || true
sleep 1

# Restart
cargo run --quiet &
SERVER_PID=$!
for i in $(seq 1 30); do curl -sf "$BASE/api/health" >/dev/null 2>&1 && break; sleep 1; done

# Verify orphan recovered to 'failed'
ORPH_STATUS=$(sqlite3 "$DB" "SELECT status FROM jobs WHERE id='$ORPH_ID'")
[ "$ORPH_STATUS" = "failed" ] || fail "recovery: expected failed, got $ORPH_STATUS"
pass "orphan recovered → failed"

echo ""
echo "RECOVERY PASSED"
RECOVERY
```

## Manual timeout verification (one-time, after applying timeout fixes)

These tests take 60–180s each. Run once to verify the fixes work, not in regular smoke runs.

```bash
# Verify reqwest timeout (fix #1): point LLM at a black-hole address.
# The job should fail within ~180s, not hang forever.
# 1. Start server with mock_llm=false and LLM_ENDPOINT pointing at nothing:
LLM_ENDPOINT=http://10.255.255.1:1/v1/chat/completions LLM_MOCK=true \
  cargo run 2>/tmp/timeout_test.log &
PID=$!; sleep 3
# 2. Enable mock for scraping, real LLM for the hang test:
curl -sf -X POST localhost:3000/api/settings/llm -H 'Content-Type: application/json' \
  -d '{"endpoint":"http://10.255.255.1:1/v1/chat/completions","api_key":"x","model":"test","openai_compat":true,"mock_llm":false}'
# 3. Submit and wait — should fail within ~180s:
JID=$(curl -sf -X POST localhost:3000/api/jobs -H 'Content-Type: application/json' \
  -d '{"url":"https://id.jobstreet.com/job/timeout-test"}' | jq -r .id)
echo "Job $JID submitted. Waiting up to 200s for timeout..."
for i in $(seq 1 40); do
  S=$(curl -sf "localhost:3000/api/jobs/$JID/card" | jq -r .status)
  [ "$S" = "failed" ] && echo "PASS: reqwest timeout fired" && break
  sleep 5
done
kill $PID 2>/dev/null; [ "$S" = "failed" ] || echo "FAIL: still $S after 200s"
```

```bash
# Verify subprocess timeout (fix #2): create a hanging mock python.
# The job should fail within ~60s, not hang forever.
MOCK_DIR=$(mktemp -d)
cat > "$MOCK_DIR/python" << 'HANGPY'
#!/bin/bash
[[ "${1:-}" == "scrape_api.py" ]] && sleep 9999 && exit 0
exec /usr/bin/python "$@"
HANGPY
chmod +x "$MOCK_DIR/python"
DB="/tmp/hangtest_$$.db"
LLM_MOCK=true DATABASE_URL="sqlite://$DB" BIND_ADDR="127.0.0.1:31997" PATH="$MOCK_DIR:$PATH" \
  cargo run &
PID=$!; sleep 3
JID=$(curl -sf -X POST http://localhost:31997/api/jobs -H 'Content-Type: application/json' \
  -d '{"url":"https://id.jobstreet.com/job/hang-test"}' | jq -r .id)
echo "Job $JID submitted with hanging scraper. Waiting up to 70s..."
for i in $(seq 1 14); do
  S=$(curl -sf "http://localhost:31997/api/jobs/$JID/card" | jq -r .status)
  [ "$S" = "failed" ] && echo "PASS: subprocess timeout fired" && break
  sleep 5
done
kill $PID 2>/dev/null; rm -f "$DB"; rm -rf "$MOCK_DIR"
[ "$S" = "failed" ] || echo "FAIL: still $S after 70s"
```
