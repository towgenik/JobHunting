# Search Approach Comparison

**Date:** 2026-07-09
**Goal:** Evaluate search approaches for job listing discovery.

---

## Comparison Matrix

| Aspect | REST v5 (`index_api.py`) | GraphQL `JobSearchV6` | GraphQL `JobCountsV6` |
|--------|--------------------------|----------------------|----------------------|
| **Endpoint** | `jobsearch-api.cloud.seek.com.au/v5/search` | `id.jobstreet.com/graphql` | `id.jobstreet.com/graphql` |
| **Auth** | No | Unknown (not tested with valid session) | No (public operation) |
| **Returns** | Full listing data (title, teaser, company, locations, classifications, salary label) | Full listing data (expected, same as REST) | Counts only (facets, no job data) |
| **Pagination** | `page` + `pageSize` (max 100) | Unknown (likely similar) | N/A |
| **Sorting** | `KeywordRelevance`, `ListedDate`, `DateUpdated` | Unknown | N/A |
| **Filtering** | keywords, siteKey, locale, dateRange, salaryRange, classification | Unknown | N/A |
| **Speed** | ~200ms/page | ~200ms (expected) | ~200ms |
| **Reliability** | Stable — public API, no auth | Unknown — UNSTABLE_QUERY_ERROR observed on JobCountsV6 | Unstable — `UNSTABLE_QUERY_ERROR` |
| **Rate limiting** | None observed at 20 rapid requests | Unknown | Unknown |
| **Implementation** | Current (`index_api.py:20-40`) | Not implemented | Not implemented |

---

## Approach Details

### 1. REST v5 Search (current: `index_api.py`)

**How it works:**
```
GET https://jobsearch-api.cloud.seek.com.au/v5/search
  ?keywords=backend+engineer&siteKey=ID&page=1&pageSize=100&locale=id-ID
```

**Response fields per job:**
- `id`, `title`, `teaser`, `companyName`
- `employer` (id, name, companyUrl)
- `locations` (label, countryCode, seoHierarchy)
- `classifications` (classification + subclassification with IDs)
- `listingDate`, `listingDateDisplay`
- `workTypes`, `salaryLabel`, `roleId`
- `bulletPoints`, `branding`, `tags`, `isFeatured`

**Tested parameters:**

| Parameter | Works | Notes |
|-----------|-------|-------|
| `keywords` | Yes | Free text |
| `siteKey` | Yes | ID, MY, AU, SG, TH, VN, PH, HK, NZ |
| `page` | Yes | 1-indexed |
| `pageSize` | Yes | **Max: 100** (400 error above) |
| `locale` | Yes | id-ID, en-ID |
| `sortMode` | Yes | KeywordRelevance, ListedDate, DateUpdated |
| `dateRange` | Yes | 1, 3, 7, 14, 30 days |
| `salaryRange` | Yes | Format: `min-max` + `salarytype=monthly` |
| `workType` | **No effect** | Accepted but doesn't filter |
| `classification` | Yes | Numeric ID (e.g., 6281 = IT & Comms) |

**Pros:**
- Zero auth — works immediately
- Rich data — everything except full description
- Stable — SEEK mobile app depends on it
- Rate-limit tolerant — 20 rapid requests all succeeded
- Max 100/page — reduces request count vs crawl_listing.py's 30/page

**Cons:**
- Teaser only — no full job descriptions
- `workType` filter broken for ID market
- VN returns AU data (fallback behavior)

**Current status:** Fully implemented in `index_api.py:20-40`. Ready to use.

---

### 2. GraphQL `JobSearchV6`

**How it works:**
- `POST https://id.jobstreet.com/graphql` with operation `JobSearchV6`
- Variables: `params`, `locale`, `timezone`
- Captured 14 times across search sessions

**Observations from captures:**
- Used alongside `JobCountsV6` on every search page
- V6 variant — `jobSearchV7` listed in SKILL.md but not captured
- Variables include structured `params` object (likely mirrors REST query params)

**Pros:**
- Potentially newer/more complete than REST v5
- Same endpoint as other GraphQL operations — unified client possible
- May support parameters REST doesn't (e.g., working workType filter)

**Cons:**
- **Not tested** — no valid session to test with
- Query structure unknown — needs fragment replication
- May require auth (viewer-specific features like saved/applied status)
- `UNSTABLE_QUERY_ERROR` observed on related `JobCountsV6` — feature flag issues?

**Current status:** Not implemented. 14 captures exist but no working query.

---

### 3. GraphQL `JobCountsV6`

**How it works:**
- `POST https://id.jobstreet.com/graphql` with operation `JobCountsV6`
- Variables: `params` only
- Returns facet counts — no job listing data

**Test results:**
- Returns `UNSTABLE_QUERY_ERROR` regardless of auth status
- Issue appears to be on JobStreet's side (feature flags), not schema-related
- 22 captures across all sessions — heavily used by frontend

**Pros:**
- Useful for progress indicators (total count before fetching)
- Lightweight — returns only counts, not listing data

**Cons:**
- **Broken** — `UNSTABLE_QUERY_ERROR` in current testing
- Counts only — cannot replace search API
- REST v5 already returns `totalCount` in search response

**Current status:** Not usable. REST v5 already provides totalCount.

---

## Recommendation

**Primary:** Keep REST v5 (`index_api.py`) as the search approach.

**Rationale:**
- Works now with zero auth
- Returns rich listing data (everything except full description)
- Max 100/page — efficient for bulk discovery
- Stable, rate-limit tolerant
- GraphQL alternatives are either untested or broken

**Improvements to `index_api.py`:**
1. Replace `crawl_listing.py` — REST API is faster (no browser) and returns more data
2. Default `pageSize=100` — reduce request count (30→100 per page)
3. Add `classification` filter for targeted searches (IT = 6281)
4. Add `salaryRange` support for salary-filtered searches

**Migration path:**
- `crawler.rs::run_search()` currently calls `crawl_listing.py` (browser-based)
- Should call `index_api.py` instead — returns structured data directly
- No need to discover URLs then scrape — search API returns enough for stub creation

---

## File References

| File | Role |
|------|------|
| `index_api.py:20-40` | REST v5 search — `search()` function |
| `index_api.py:43-103` | Multi-page indexer — `index()` function |
| `index_api.py:106-123` | URL→keywords extractor — `extract_keywords_from_url()` |
| `crawler.rs:run_search()` | Rust caller — currently uses `crawl_listing.py` |
| `crawl_listing.py` | Legacy browser-based URL discovery |
| `docs/test-results/public-api.md` | Full REST API test results |
| `docs/graphql-operations/operation-summary.md` | GraphQL operation inventory |
