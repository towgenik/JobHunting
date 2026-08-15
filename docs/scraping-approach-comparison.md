# Job Detail Scraping — Approach Comparison

**Date:** 2026-07-09
**Goal:** Replace or improve `scrape_api.py` for fetching full job descriptions.

---

## Comparison Matrix

| Aspect | scrape_api.py (HTML regex) | GraphQL `jobDetails` (NEW) | GraphQL `jobDetailsPersonalised` | SSR JSON extraction | Public REST API |
|--------|---------------------------|----------------------------|----------------------------------|---------------------|-----------------|
| **Auth needed** | No | Yes (Bearer JWT) | Yes (Bearer JWT) | No | No |
| **Fields available** | title, description, company | title, abstract, content2 (full HTML), classification, location, workType, expiry, verified | saved status, applied date, salary match | Structured from page hydration JSON | Teaser only |
| **Speed** | ~1s | ~200ms | ~200ms | ~500ms | ~200ms |
| **Reliability** | Fragile (regex drift) | **Stable** (API contract) | Stable (API contract) | Medium | Stable |
| **Anti-bot risk** | Medium | Low | Low | Medium | None |
| **Implementation** | Current | **Recommended** | Supplementary | Unimplemented | Already done |
| **Query size** | N/A | 5,058 chars | 1,753 chars | N/A | N/A |
| **Response size** | ~10-30KB HTML | ~12KB JSON | ~552B JSON | ~10-30KB | ~2KB |

**Winner: GraphQL `jobDetails`** — returns full structured data including `content2` (full HTML description) without fragile regex parsing. Auth is the only barrier, but token extraction from browser localStorage is feasible.

---

## Approach Details

### 1. HTML Regex (current: `scrape_api.py`)

**How it works:**
- `GET <job-url>` with User-Agent header
- Regex on `data-automation="job-detail-title"` for title
- Regex on `data-automation="advertiser-name"` for company
- Regex from `data-automation="jobAdDetails"` to `report-job-ad-toggle` for description

**Pros:**
- Zero auth, zero cookies, zero dependencies
- Works immediately — no setup
- ~1s per job is acceptable for sequential processing

**Cons:**
- `data-automation` attributes are internal — JobStreet can change them without notice
- Regex boundary (`report-job-ad-toggle`) is fragile
- Cloudflare may challenge after sustained scraping
- No structured data — just raw text extraction

**Current status:** Working. Used by `generate.rs::fetch_job()`.

---

### 2. GraphQL `jobDetailsPersonalised`

**How it works:**
- `POST https://id.jobstreet.com/graphql` with operation `jobDetailsPersonalised`
- Requires full fragment replication (`jobPersonalised` fragment)
- Returns structured JSON: title, description, salary, location, classifications, etc.

**Pros:**
- Rich structured data — salary info, classifications, work arrangements
- API contract — SEEK frontend depends on it, changes are versioned
- Fast — small JSON response vs full HTML page

**Cons:**
- Requires authenticated session (cookies + possibly Bearer token)
- Cookie refresh needed every ~30 min (`__cfbm` expiry)
- Query replication is complex — 400 errors on incomplete fragments
- `UNSTABLE_QUERY_ERROR` observed on some operations (feature flags?)
- Auth flow: login.seek.com OAuth → token exchange → cookie setup

**Current status:** Not implemented. Captured via mitmproxy but not tested with valid session.

---

### 3. SSR JSON Extraction

**How it works:**
- `GET <job-url>` — same as HTML approach
- Parse `<script>` tags containing `SEEK_CONFIG` or React hydration data
- Extract job data from embedded JSON object

**Pros:**
- No auth needed
- Structured data — JSON has typed fields, not regex-matched text
- Faster than regex — one JSON parse vs multiple regex passes

**Cons:**
- JSON embedding location is undocumented — may change with frontend deploys
- Need to find correct `<script>` tag among many
- Parsing overhead for large HTML pages
- Same Cloudflare risk as HTML approach

**Current status:** Not implemented. Mentioned in SKILL.md as "Option C" but no code exists.

---

### 4. Public REST API

**How it works:**
- `GET https://jobsearch-api.cloud.seek.com.au/v5/search` — already in `index_api.py`
- Returns teaser text only — no full job descriptions

**Pros:**
- No auth, no cookies, no browser
- Stable API contract (SEEK mobile app uses it)
- Fast, rate-limit tolerant

**Cons:**
- **No detail endpoint exists** — search returns teasers, not full descriptions
- Cannot replace detail scraping — complements it

**Current status:** Used for search (`index_api.py`). Cannot be used for detail scraping.

---

## Recommendation

**Primary:** Keep `scrape_api.py` (HTML regex) as the main approach.

**Rationale:**
- It works now with zero setup
- The other approaches either need auth (GraphQL), are unimplemented (SSR JSON), or lack detail data (REST API)
- For a single-user tool processing ~50 jobs/day, 1s/job is fine
- Fragility is manageable — add a validation step that checks extracted text length

**Improvement path:**
1. Add SSR JSON extraction as fallback when regex returns empty fields
2. Monitor `data-automation` attribute stability — log warnings on empty extractions
3. If Cloudflare blocks become frequent, switch to `StealthyFetcher` (already supported in scrape.py)

---

## File References

| File | Role |
|------|------|
| `scrape_api.py:18-64` | HTML regex scraper — `scrape()` function |
| `generate.rs:fetch_job()` | Rust caller — invokes `scrape_api.py` as subprocess |
| `scrape.py` | Legacy headless browser scraper (Scrapling) |
| `index_api.py` | REST API search — no detail descriptions |
| `.claude/skills/scrape/SKILL.md` | Full pipeline documentation |
