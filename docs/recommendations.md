# Scraping Pipeline — Recommendations

**Date:** 2026-07-09
**Source:** Evaluation of search, detail scraping, and auth approaches.

---

## Summary of Recommendations

| Component | Recommendation | Reason |
|-----------|---------------|--------|
| **Job detail scraping** | **GraphQL `jobDetails`** (new finding) | Returns structured data + full HTML description, no regex parsing needed |
| **Search** | Replace `crawl_listing.py` with `index_api.py` | Faster, richer data, no browser needed |
| **Auth strategy** | Bearer JWT from localStorage | Required for GraphQL jobDetails |
| **Priority 1** | Implement `jobDetails` GraphQL scraper | Biggest win — replaces fragile HTML regex |
| **Priority 2** | Integrate `index_api.py` into Rust crawler | Eliminates browser from search |
| **Priority 3** | Keep `scrape_api.py` as fallback | Resilience if GraphQL fails |

---

## 1. Job Detail Scraping

**Decision:** Use GraphQL `jobDetails` operation as primary approach.

**New finding:** The `jobDetails` GraphQL query returns:
- `job.title` — job title
- `job.abstract` — short description
- `job.content2` — **full HTML description** (the key field)
- `job.tracking.classificationInfo` — industry + sub-industry
- `job.tracking.locationInfo` — city/area
- `job.tracking.workTypeIds` — work type
- `job.isExpired`, `job.expiresAt` — expiry info
- `job.isVerified` — verified employer

**Query saved:** `docs/graphql-operations/jobDetails-query.graphql` (5,058 chars)
**Response saved:** `docs/graphql-operations/jobDetails-response.json` (12KB)

**Auth requirement:** Bearer JWT from `login.seek.com/oauth/token` (15min expiry). Token stored in browser localStorage at `@@auth0spajs@@::8OVhpvtaI9n5QVEQK3X5yfsmCbrrLXfE::@@user@@`.

**Implementation approach:**
1. Extract Bearer token from browser localStorage via CDP
2. POST to `https://id.jobstreet.com/graphql` with `Authorization: Bearer <token>`
3. Parse `content2` HTML field (strip tags, normalize whitespace)
4. No regex parsing of full page HTML needed

**Fallback:** Keep `scrape_api.py` (HTML regex) if GraphQL fails (token expired, rate limited, etc.).

---

## 2. Search

**Decision:** Replace `crawl_listing.py` with `index_api.py` in the Rust pipeline.

**Current flow:**
```
crawler.rs::run_search()
  → crawl_listing.py <url>          # Browser-based, discovers URLs only
  → for each URL:
      → create_job_stub_for_search()  # DB stub
      → process_job()                 # scrape_api.py + LLM
```

**Proposed flow:**
```
crawler.rs::run_search()
  → index_api.py <keywords> --pages 5 --page-size 100  # REST API, returns structured data
  → for each job:
      → create_job_stub_with_data()   # DB stub with title, company, teaser, etc.
      → process_job()                 # scrape_api.py + LLM (only if needed)
```

**Benefits:**
- Eliminates browser from search path — `crawl_listing.py` uses Scrapling DynamicFetcher
- Returns structured data immediately — title, company, locations, classifications, salary
- 100 items/page vs 30 — fewer requests
- `totalCount` available for progress tracking
- `dateRange` and `classification` filters for targeted searches

**Changes needed:**
1. `src/crawler.rs` — `run_search()` calls `index_api.py` instead of `crawl_listing.py`
2. `index_api.py` — add `--json-output` mode for Rust subprocess parsing
3. DB schema — store teaser, company, locations from search response (currently only URL)

**Files:**
- `index_api.py:43-103` — `index()` function
- `src/crawler.rs` — `run_search()` function
- `crawl_listing.py` — to be deprecated

---

## 3. Auth Strategy

**Decision:** Keep cookie-based auth. Do not implement OAuth.

**Current auth usage:**

| Component | Auth needed | Notes |
|-----------|------------|-------|
| `index_api.py` (REST search) | No | Public API |
| `scrape_api.py` (HTML scraping) | No | Public pages |
| GraphQL public ops | No | GetBanner, getClassificationOptions, etc. |
| GraphQL viewer ops | Cookies | Profile data — not in scraping pipeline |
| `/api/jobsearch/*` | Bearer token | Unclear purpose — not needed |

**Why not OAuth:**
- Core pipeline works without any auth
- OAuth adds complexity (token storage, refresh, client_id management)
- `/api/jobsearch/persist` and `/api/jobsearch/unpersist` are not needed for scraping
- Profile viewer queries are out of scope for the scraping pipeline

**If GraphQL becomes needed:**
- Refresh cookies via `session.py` (existing mechanism)
- Test if cookies work for GraphQL operations (untested with valid session)
- Only implement OAuth if cookies fail for GraphQL

**Files:**
- `session.py` — cookie harvest
- `docs/test-results/auth.md` — full auth analysis

---

## 4. Migration Path

### Phase 1: Integrate REST API into crawler (Priority 1)

**Goal:** Replace browser-based search with REST API.

1. Modify `src/crawler.rs::run_search()` to call `index_api.py`
2. Parse JSON output from `index_api.py`
3. Store search metadata (title, company, teaser) in DB stubs
4. Keep `scrape_api.py` for full description fetching
5. Deprecate `crawl_listing.py`

**Effort:** ~2 hours (Rust subprocess integration + JSON parsing)
**Risk:** Low — `index_api.py` already works, just needs wiring

### Phase 2: Add scraping resilience (Priority 2)

**Goal:** Detect and handle selector drift.

1. Add validation to `scrape_api.py` — reject empty/short descriptions
2. Log extraction quality metrics (field lengths, empty counts)
3. Implement SSR JSON extraction as fallback parser
4. Add retry with fallback: regex → SSR JSON → error

**Effort:** ~3 hours (SSR JSON parser + fallback logic)
**Risk:** Low — additive change, existing regex stays primary

### Phase 3: GraphQL integration (Priority 3 — optional)

**Goal:** Use GraphQL for richer data when auth is available.

1. Test `jobDetailsPersonalised` with valid cookies
2. Replicate query fragments from captured data
3. Add GraphQL client to `scrape_api.py` as optional mode
4. Fall back to HTML regex when GraphQL fails

**Effort:** ~6 hours (query replication + auth handling + fallback)
**Risk:** Medium — auth expiry, query stability, feature flags

---

## 5. Priority Order

| Priority | Task | Impact | Effort | Dependencies |
|----------|------|--------|--------|-------------|
| **P1** | Integrate `index_api.py` into `crawler.rs` | High — eliminates browser from search | 2h | None |
| **P2** | Add scraping validation + SSR JSON fallback | Medium — resilience against drift | 3h | None |
| **P3** | GraphQL `jobDetailsPersonalised` | Low — richer data, but needs auth | 6h | Valid session testing |
| **P4** | Replace `crawl_listing.py` entirely | Low — cleanup after P1 | 1h | P1 complete |
| **P5** | OAuth implementation | Very low — no current need | 8h | Only if GraphQL requires it |

---

## 6. What NOT to Do

- **Don't implement OAuth** — core pipeline doesn't need it
- **Don't rewrite scrape_api.py** — it works, improve incrementally
- **Don't add browser automation for search** — REST API is faster and more reliable
- **Don't commit `session.json`** — already in `.gitignore`, keep it that way
- **Don't use GraphQL for search** — REST v5 is stable, tested, and auth-free

---

## File References

| File | Current Role | Recommendation |
|------|-------------|----------------|
| `index_api.py` | Standalone search tool | Integrate into crawler pipeline |
| `scrape_api.py` | Detail scraper | Keep, add validation + fallback |
| `crawl_listing.py` | Browser-based URL discovery | Deprecate after P1 |
| `session.py` | Cookie harvest | Keep as-is |
| `src/crawler.rs` | Calls crawl_listing.py | Switch to index_api.py |
| `src/generate.rs` | Calls scrape_api.py | No change needed |
| `.claude/skills/scrape/SKILL.md` | Pipeline docs | Update after P1 |
