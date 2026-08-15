# JobStreet API Research Notes

**Date started:** 2026-07-08
**Tool:** mitmproxy 12.2.3 + mitmproxy2swagger
**Target:** https://id.jobstreet.com/

## Goal

Reverse-engineer JobStreet's full API surface by intercepting browser traffic with mitmproxy, then generate OpenAPI spec with mitmproxy2swagger.

## What Already Known (from scrape SKILL.md)

- Public REST API: `https://jobsearch-api.cloud.seek.com.au/v5/search` — no auth needed
- GraphQL endpoint: `https://id.jobstreet.com/graphql` — needs login cookies
- GraphQL operations: jobSearchV7, JobCountV7, jobDetailsPersonalised, etc.
- HTML scraping works but fragile (CSS selectors on detail pages)

## Capture Setup

### Architecture
```
KasmVNC Chrome (container) --proxy--> mitmdump (host:8080) --writes--> capture.flow
                                                                        |
                                                                        v
                                                              mitmproxy2swagger --> OpenAPI spec
```

### mitmdump running
```bash
mitmdump -w /tmp/jobstreet-research/capture.flow
# PID: 514402, port 8080
```

### Browser proxy config
- Chrome in KasmVNC configured with `--proxy-server=http://172.18.0.1:8080 --ignore-certificate-errors`
- Added to login/Dockerfile and rebuilt container
- CA cert at `~/.mitmproxy/mitmproxy-ca-cert.pem` (not needed due to --ignore-certificate-errors)
- **Note:** `host.docker.internal` didn't resolve; used Docker gateway IP `172.18.0.1` instead

## Findings Log

### FAILED attempts
<!-- Log everything that didn't work -->

1. **ERR_PROXY_CONNECTION_FAILED** — container couldn't reach host proxy
   - Cause: UFW firewall blocking Docker network (172.18.0.0/16) → host:8080
   - Fix: `sudo ufw allow from 172.16.0.0/12 to any port 8080`
   - Also: `host.docker.internal` didn't resolve; used gateway IP `172.18.0.1` instead

2. **Empty mitmproxy2swagger output** — spec.yaml had no paths
   - Cause: Only Chrome startup traffic captured, no JobStreet browsing yet
   - Fix: Restart mitmdump clean, browse JobStreet

### WORKING discoveries
<!-- Log what works -->

1. **mitmdump proxy capture works** — 23MB captured, 2438 lines of log
2. **Chrome proxy via gateway IP** — `172.18.0.1:8080` works from container
3. **mitmproxy2swagger** — auto-generated path templates from capture
4. **browser-harness** — CDP automation works for browsing sessions
5. **session.py fix** — now harvests cookies from all 3 domains (jobstreet.com, id.jobstreet.com, login.seek.com)
6. **47 unique GraphQL operations** discovered (38 new vs SKILL.md)
7. **Bearer token auth** — GraphQL requires `Authorization: Bearer <JWT>` from `login.seek.com/oauth/token` (15min expiry)

### API Endpoints Discovered
<!-- Table of endpoints found via mitmproxy -->

| Method | URL | Auth? | Purpose | Status |
|--------|-----|-------|---------|--------|
| GET | https://jobsearch-api.cloud.seek.com.au/v5/search | No | Job search (pageSize max=100) | Known |
| POST | https://id.jobstreet.com/graphql | Bearer JWT | 47 GraphQL operations | **Expanded** |
| **POST** | **https://login.seek.com/oauth/token** | client_id | **OAuth token exchange** | **NEW** |
| GET | https://login.seek.com/time | No | Time sync (unix timestamp) | NEW |
| GET | https://login.seek.com/v2/logout | Yes | Logout | NEW |
| **GET** | **https://id.jobstreet.com/api/jobsearch/persist** | Bearer JWT | **Persist search state** | **NEW (401 without Bearer)** |
| **GET** | **https://id.jobstreet.com/api/jobsearch/unpersist** | Bearer JWT | **Unpersist search state** | **NEW (401 without Bearer)** |
| GET | https://id.jobstreet.com/oauth-ssr/login | No | SSR login page | NEW |
| GET | https://id.jobstreet.com/oauth-ssr/callback | No | OAuth callback | NEW |
| GET | https://id.jobstreet.com/oauth-ssr/logout | Yes | SSR logout | NEW |
| GET | https://id.jobstreet.com/id/oauth/login | No | OAuth login redirect | NEW |
| GET | https://id.jobstreet.com/id/oauth/callback | No | OAuth callback | NEW |
| GET | https://id.jobstreet.com/id/oauth/logout | Yes | OAuth logout | NEW |
| POST | https://seek-metrics-forwarder.cloud.seek.com.au/v1/send | No | Analytics/metrics (204) | NEW |

### Request/Response Patterns
<!-- Document headers, cookies, tokens needed -->

**OAuth flow observed:**
1. `login.seek.com/time` — time sync (called many times, NTP-like)
2. `login.seek.com/oauth/token` — POST, token exchange (15min JWT)
3. `login.seek.com/v2/logout` — logout with `client_id` and `returnTo` params

**GraphQL auth requirements:**
- Public operations (no auth): `GetBanner`, `getKeywordSuggestions`, `getClassificationOptions`, `GetProfileVisibilityOptions`, `GetWorkTypeOptions`, `GetSupportedCountries`
- Authenticated operations (Bearer JWT): `getUserInfo`, `GetProfile`, `GetSkills`, `GetPersonalDetails`, `GetResumes`, etc.
- Token stored in localStorage: `@@auth0spajs@@::8OVhpvtaI9n5QVEQK3X5yfsmCbrrLXfE::@@user@@`
- Token format: JWT with claims for seek (user_id, country, brand, experience)

**New API endpoints:**
- `/api/jobsearch/persist` and `/api/jobsearch/unpersist` — search state persistence, require Bearer token

**Other services discovered:**
- `seek-metrics-forwarder.cloud.seek.com.au` — analytics (204 No Content)
- `image-service-cdn.seek.com.au` — image CDN
- `bx-branding-gateway.cloud.seek.com.au` — branding config (401)
- `tracking.engineering.cloud.seek.com.au` — internal tracking (200)

### Capture files
- `docs/jobstreet-capture.flow` (23MB) — initial capture
- `docs/captures/all-sessions.flow` (18MB) — automated CDP capture with 50 GraphQL ops
- `docs/captures/manual-browse.flow` (85MB) — manual browse with 310 GraphQL ops
- `docs/captures/manual-browse-2.flow` (21MB) — manual browse 2 with 62 GraphQL ops
- **Total: 147MB captured, 548 GraphQL operations, 61 unique**
- `docs/graphql-operations/*.jsonl` — extracted GraphQL operations per session

### Key Manual Browse Findings

The 85MB manual browse captured the critical operations we were missing:

| Operation | Count | Response Size | Purpose |
|-----------|-------|---------------|---------|
| `jobDetails` | 5x | **12KB** | **Full job data: title, content2 (HTML description), classification, location** |
| `jobDetailsPersonalised` | 7x | 552B | Saved status, salary match |
| `JobDetailsRecommendedJobs` | 7x | 2.9KB | Related jobs |
| `GetMatchedQualities` | 6x | 2.5KB | Skills match |
| `JobSearchV6` | 60x | 78KB | Search results |
| `JobCountV7` | 60x | 202B | Job counts |

**Critical finding:** `jobDetails` returns `content2` field with full HTML description. This replaces the need for regex scraping of HTML pages.

## Final Status

- [x] Install mitmproxy2swagger
- [x] Start mitmdump capture on host:8080
- [x] Start KasmVNC container for login
- [x] Configure Chrome proxy in Dockerfile
- [x] User logs in via noVNC at http://localhost:6901
- [x] Browse JobStreet (search, detail, apply) to generate traffic
- [x] Run mitmproxy2swagger on captured flows
- [x] Document new endpoints found
- [x] Test new endpoints with curl/reqwest
- [x] Generate full OpenAPI spec
- [x] Investigate `/api/jobsearch/persist` and `/api/jobsearch/unpersist`
- [x] Capture GraphQL request/response bodies for operation details

## Documentation Created

| File | Description |
|------|-------------|
| `docs/API-REFERENCE-vol1.md` | Public REST API (jobsearch-api.cloud.seek.com.au) |
| `docs/API-REFERENCE-vol2.md` | GraphQL API (id.jobstreet.com/graphql) |
| `docs/API-REFERENCE-vol3.md` | Internal endpoints (id.jobstreet.com/api/*) |
| `docs/API-REFERENCE-vol4.md` | Auth endpoints (login.seek.com) |
| `docs/API-REFERENCE-vol5.md` | Other services (metrics, CDN, branding) |
| `docs/GRAPHQL-REFERENCE.md` | GraphQL schema, 47 operations |
| `docs/AUTH-GUIDE.md` | OAuth flow, token lifecycle, Bearer auth |
| `docs/SERVICE-MAP.md` | SEEK platform architecture |
| `docs/scraping-approach-comparison.md` | HTML regex vs GraphQL vs REST |
| `docs/search-approach-comparison.md` | REST v5 vs GraphQL JobSearchV6 |
| `docs/auth-strategy-evaluation.md` | Cookie vs OAuth vs Hybrid |
| `docs/recommendations.md` | Final recommendations |
| `docs/graphql-operations/*.jsonl` | Captured GraphQL operations |
| `docs/test-results/*.md` | Endpoint test results |
