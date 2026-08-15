# API Reference Vol 1 — Public REST API

**Base URL:** `https://jobsearch-api.cloud.seek.com.au/v5`
**Last updated:** 2026-07-08
**Auth required:** No

---

## Table of Contents

- [§1 Overview](#§1-overview)
- [§2 Authentication](#§2-authentication)
- [§3 Endpoints](#§3-endpoints)
  - [§3.1 GET /search — Job Search](#§31-get-search--job-search)
- [§4 Response Schema](#§4-response-schema)
  - [§4.1 Top-Level Response](#§41-top-level-response)
  - [§4.2 Job Object (data array item)](#§42-job-object-data-array-item)
  - [§4.3 Nested Objects](#§43-nested-objects)
- [§5 Error Codes](#§5-error-codes)
- [§6 Rate Limits](#§6-rate-limits)
- [§7 Cross-References](#§7-cross-references)

---

## §1 Overview

Public REST API for job search across SEEK/JobStreet markets. No authentication,
session, or cookies required. Returns JSON. Designed for the SEEK React frontend
but accessible to any HTTP client.

**Supported markets:** Indonesia (ID), Malaysia (MY), Australia (AU), Singapore (SG),
Thailand (TH), Vietnam (VN), Philippines (PH), Hong Kong (HK), New Zealand (NZ).

**Key characteristics:**
- Teaser-only results (no full job descriptions)
- Max 100 items per page
- No rate limiting observed at moderate load (20 rapid requests)
- Sorting, filtering by date/salary/classification supported
- `workType` parameter accepted but does NOT filter results

**Related APIs:**
- GraphQL API (full job details, profile ops): [Vol2 §1](API-REFERENCE-vol2.md#§1-overview)
- Internal endpoints: [Vol3 §1](API-REFERENCE-vol3.md#§1-overview)
- Auth endpoints: [Vol4 §1](API-REFERENCE-vol4.md#§1-overview)

---

## §2 Authentication

**None required.** This is a fully public API. No API keys, tokens, cookies,
or User-Agent requirements observed.

Tested: 20 rapid sequential requests all returned HTTP 200 with no auth headers.

---

## §3 Endpoints

### §3.1 GET /search — Job Search

**Full URL:** `https://jobsearch-api.cloud.seek.com.au/v5/search`
**Method:** GET
**Auth:** None
**Content-Type:** N/A (query params only)

#### §3.1.1 Query Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `keywords` | string | No | `""` | Free-text search query |
| `siteKey` | string | Yes | — | Country code (see valid values below) |
| `page` | int | No | `1` | Page number (1-indexed) |
| `pageSize` | int | No | `22` | Results per page. **Max: 100** (HTTP 400 above) |
| `locale` | string | No | — | Locale for display language (e.g., `id-ID`, `en-ID`) |
| `sortMode` | string | No | `KeywordRelevance` | Sort order (see valid values below) |
| `dateRange` | int | No | — | Filter by listing age in days |
| `salaryRange` | string | No | — | Salary range filter (format: `min-max`) |
| `salarytype` | string | No | — | Salary period (e.g., `monthly`) |
| `classification` | int | No | — | Industry classification ID |
| `workType` | string | No | — | Work type filter (accepted but **no effect** in ID market) |

#### §3.1.2 Valid siteKey Values

| siteKey | Country | Observed totalCount |
|---------|---------|---------------------|
| `ID` | Indonesia | 8,826 |
| `MY` | Malaysia | 14,491 |
| `AU` | Australia | 22,179 |
| `SG` | Singapore | 21,929 |
| `TH` | Thailand | 5,473 |
| `VN` | Vietnam | 22,179 (likely falls back to AU) |
| `PH` | Philippines | 13,497 |
| `HK` | Hong Kong | 6,916 |
| `NZ` | New Zealand | 4,305 |

#### §3.1.3 Valid sortMode Values

| sortMode | Label (id-ID) | Description |
|----------|---------------|-------------|
| `KeywordRelevance` | Relevansi | Default. Relevance to keywords |
| `ListedDate` | Tanggal | Sort by listing date |
| `DateUpdated` | — | Sort by last update date |

#### §3.1.4 Valid dateRange Values

| dateRange | Description | Example totalCount (ID) |
|-----------|-------------|-------------------------|
| `1` | Last 24 hours | 5 |
| `3` | Last 3 days | 1,595 |
| `7` | Last 7 days | 2,460 |
| `14` | Last 14 days | 4,205 |
| `30` | Last 30 days | 7,782 |
| (omit) | All time | 8,826 |

Cumulative filter — includes all jobs listed within the range.

#### §3.1.5 Valid locale Values

| locale | Language | Notes |
|--------|----------|-------|
| `id-ID` | Bahasa Indonesia | Default for ID market |
| `en-ID` | English | Same results, different display strings |

#### §3.1.6 salaryRange Format

```
salaryRange=5000000-10000000&salarytype=monthly
```

Uses local currency (IDR for ID, AUD for AU, etc.). Filters ~19% of results
in tested example.

#### §3.1.7 classification Filter

Numeric ID from classification data. Example: `classification=6281` →
IT & Communications (Teknologi Informasi & Komunikasi), 2,049 results.

Get classification IDs from response `data[].classifications` or via
GraphQL `getClassificationOptions` operation
[see Vol2 §3.4](API-REFERENCE-vol2.md#§34-utility-operations).

#### §3.1.8 pageSize Limits

| pageSize | Result | HTTP Status |
|----------|--------|-------------|
| 22 | 22 items | 200 |
| 50 | 50 items | 200 |
| 100 | 100 items | 200 |
| 150 | 0 items | **400** |
| 200 | 0 items | **400** |

**Maximum pageSize: 100.** Values >100 return HTTP 400 with empty body.

#### §3.1.9 Example Request

```bash
curl "https://jobsearch-api.cloud.seek.com.au/v5/search?keywords=backend+engineer&siteKey=ID&page=1&pageSize=22&locale=id-ID"
```

#### §3.1.10 Pagination

Use `page` parameter (1-indexed). Total pages = `ceil(totalCount / pageSize)`.
The `totalCount` field in response gives exact total for progress tracking.

---

## §4 Response Schema

### §4.1 Top-Level Response

```json
{
  "data": [ /* job objects */ ],
  "totalCount": 8826,
  "info": {
    "timeTaken": 42,
    "source": "...",
    "experiment": "...",
    "newSince": "..."
  },
  "userQueryId": "uuid-string",
  "sortModes": [
    {
      "name": "KeywordRelevance",
      "label": "Relevansi",
      "isActive": true
    }
  ],
  "solMetadata": {
    "requestToken": "...",
    "facetCounts": { ... }
  },
  "facets": { },
  "searchParams": {
    "keywords": "backend engineer",
    "siteKey": "ID",
    "page": "1",
    "pageSize": "22",
    "locale": "id-ID"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `data` | array | Job listing objects (up to pageSize) |
| `totalCount` | int | Total matching jobs across all pages |
| `info` | object | Request metadata (timing, source, experiment) |
| `userQueryId` | string | Unique request identifier |
| `sortModes` | array | Available sort options with active state |
| `solMetadata` | object | Search engine metadata, request token, facet counts |
| `facets` | object | Filter facets (empty in basic queries) |
| `searchParams` | object | Echo of query parameters sent |

### §4.2 Job Object (data array item)

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Job ID (e.g., `"93021144"`) |
| `title` | string | Job title |
| `teaser` | string | Short description snippet |
| `companyName` | string | Company name |
| `advertiser` | object | `{id, description}` — advertiser info |
| `employer` | object | `{id, name, companyId, companyUrl}` |
| `listingDate` | string | ISO 8601 timestamp (e.g., `"2026-06-22T09:29:36Z"`) |
| `listingDateDisplay` | string | Localized relative date (e.g., `"16 jam yang lalu"`) |
| `salaryLabel` | string | Formatted salary string (empty if not specified) |
| `workTypes` | array | e.g., `["Full time"]` |
| `workArrangements` | string | Work arrangement label (e.g., `"Kantor"` = Office) |
| `locations` | array | Location objects with `label`, `countryCode`, `seoHierarchy` |
| `classifications` | array | Industry + subclassification with IDs |
| `roleId` | string | SEO-friendly role slug (e.g., `"backend-engineer"`) |
| `bulletPoints` | array | Highlight strings |
| `branding` | object | Company logo URL |
| `tags` | array | e.g., `[{type: "URGENT", label: "Dibutuhkan segera"}]` |
| `isFeatured` | boolean | Whether job is featured |
| `displayType` | string | `"standard"` or other |
| `tracking` | string | Base64 tracking token |
| `solMetadata` | object | Per-job search metadata |

### §4.3 Nested Objects

#### §4.3.1 employer Object

```json
{
  "id": "433606",
  "name": "Indonesia Fintopia Technology",
  "companyId": "433606",
  "companyUrl": "https://..."
}
```

#### §4.3.2 locations Array Item

```json
{
  "label": "Jakarta Raya",
  "countryCode": "ID",
  "seoHierarchy": ["Indonesia", "Jakarta", "Jakarta Raya"]
}
```

#### §4.3.3 classifications Array Item

```json
{
  "classification": {
    "id": "6281",
    "description": "Information & Communication Technology"
  },
  "subclassification": {
    "id": "628100",
    "description": "Engineering - Software"
  }
}
```

#### §4.3.4 branding Object

Company logo image URL, sourced from SEEK image CDN
(`image-service-cdn.seek.com.au`).

---

## §5 Error Codes

| HTTP Status | Condition | Body |
|-------------|-----------|------|
| 200 | Success | JSON response |
| 400 | Invalid parameters (e.g., pageSize > 100) | Empty or error message |

No 401, 403, 429 responses observed in testing.

---

## §6 Rate Limits

**No rate limiting detected.** 20 rapid sequential requests all returned HTTP 200.

Recommendation: Add 200-500ms delay between pages for politeness. The API is
designed for the SEEK React frontend which paginates aggressively.

---

## §7 Cross-References

- **GraphQL search alternative:** `JobSearchV6` operation — [Vol2 §3.1](API-REFERENCE-vol2.md#§31-search-operations)
- **Job counts via GraphQL:** `JobCountsV6`, `JobCountV7` — [Vol2 §3.1](API-REFERENCE-vol2.md#§31-search-operations)
- **Classification taxonomy:** `getClassificationOptions` GraphQL — [Vol2 §3.4](API-REFERENCE-vol2.md#§34-utility-operations)
- **Full job descriptions:** Requires GraphQL `jobDetailsPersonalised` or HTML scraping — [Scrape SKILL](../.claude/skills/scrape/SKILL.md)
- **Auth for other endpoints:** [Vol4 §2](API-REFERENCE-vol4.md#§2-oauth-flow)
- **All domains:** [Service Map §1](SERVICE-MAP.md#§1-domains-and-roles)
