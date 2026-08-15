# API Reference Vol 3 — Internal Endpoints

**Base URL:** `https://id.jobstreet.com`
**Last updated:** 2026-07-08

---

## Table of Contents

- [§1 Overview](#§1-overview)
- [§2 Authentication](#§2-authentication)
- [§3 Endpoints](#§3-endpoints)
  - [§3.1 GET /api/jobsearch/persist](#§31-get-apijobsearchpersist)
  - [§3.2 GET /api/jobsearch/unpersist](#§32-get-apijobsearchunpersist)
  - [§3.3 OAuth SSR Endpoints](#§33-oauth-ssr-endpoints)
  - [§3.4 OAuth SPA Endpoints](#§34-oauth-spa-endpoints)
- [§4 Cross-References](#§4-cross-references)

---

## §1 Overview

Internal endpoints on `id.jobstreet.com` for search state management and OAuth flow.
Not part of the public API surface. Serve both SSR (server-side rendered) and SPA
(single-page application) routes.

**Key finding:** `/api/jobsearch/*` endpoints require OAuth Bearer token, NOT cookies.
Cookies alone are insufficient for these API calls.

---

## §2 Authentication

Two auth modes on this domain:

| Path Pattern | Auth Method | Notes |
|--------------|-------------|-------|
| `/api/jobsearch/*` | OAuth Bearer token | `Authorization: Bearer <token>` required |
| `/oauth-ssr/*` | None (redirects) | Server-side redirects, no auth needed |
| `/id/oauth/*` | None (SPA pages) | Serve HTML pages, no auth needed |
| `/graphql` | Session cookies | Cookie-based auth |

### §2.1 Bearer Token Requirement

Tested with full session cookies:
```
GET /api/jobsearch/persist → HTTP 401
Body: "OAuth Denied: Authorization header missing"
```

**Conclusion:** These endpoints require `Authorization: Bearer <token>` header.
Token obtained via OAuth flow — see [AUTH-GUIDE.md §1](AUTH-GUIDE.md#§1-oauth-flow-diagram)
and [Vol4 §3.2](API-REFERENCE-vol4.md#§32-post-oauthtoken).

---

## §3 Endpoints

### §3.1 GET /api/jobsearch/persist

| Field | Value |
|-------|-------|
| **Full URL** | `https://id.jobstreet.com/api/jobsearch/persist` |
| **Method** | GET |
| **Auth** | OAuth Bearer token required |
| **HTTP Status (no auth)** | 401 |
| **Response (no auth)** | `"OAuth Denied: Authorization header missing"` |

Persists search state for the authenticated user. Likely saves current search
parameters (keywords, filters, location) for cross-session continuity.

**Required headers:**
```
Authorization: Bearer <token>
```

**Use case:** Maintain search context across page loads. Could be useful for
maintaining search state in scraper sessions.

### §3.2 GET /api/jobsearch/unpersist

| Field | Value |
|-------|-------|
| **Full URL** | `https://id.jobstreet.com/api/jobsearch/unpersist` |
| **Method** | GET |
| **Auth** | OAuth Bearer token required |
| **HTTP Status (no auth)** | 401 |
| **Response (no auth)** | `"OAuth Denied: Authorization header missing"` |

Clears persisted search state. Counterpart to `/api/jobsearch/persist`.

### §3.3 OAuth SSR Endpoints

Server-side rendered OAuth flow endpoints. These return HTTP redirects (302)
and serve as the backend for the OAuth authorization code flow.

| Endpoint | Method | HTTP Status | Description |
|----------|--------|-------------|-------------|
| `/oauth-ssr/login` | GET | 302 | Initiates SSR login redirect |
| `/oauth-ssr/callback` | GET | 302 | OAuth callback handler |
| `/oauth-ssr/logout` | GET | 302 | SSR logout redirect |
| `/oauth-ssr/allow-redirect` | GET | 200 | CORS/redirect validation page |
| `/oauth-ssr/post-logout-redirect` | GET | 302 | Post-logout redirect |

**Flow sequence:**
1. `/oauth-ssr/login` → redirects to `login.seek.com/authorize`
2. User authenticates at login.seek.com
3. `/oauth-ssr/callback` → receives authorization code
4. Token exchange at `login.seek.com/oauth/token`
5. `/oauth-ssr/logout` → clears session → `/oauth-ssr/post-logout-redirect`

See [AUTH-GUIDE.md §1](AUTH-GUIDE.md#§1-oauth-flow-diagram) for full flow diagram.

### §3.4 OAuth SPA Endpoints

Single-page application routes that serve HTML pages for the client-side OAuth flow.

| Endpoint | Method | HTTP Status | Description |
|----------|--------|-------------|-------------|
| `/id/oauth/login` | GET | 200 | SPA login page |
| `/id/oauth/callback` | GET | 200 | SPA callback page (receives code) |
| `/id/oauth/logout` | GET | 200 | SPA logout page |
| `/id/oauth/redirect` | GET | 200 | SPA redirect helper page |

**Full OAuth sequence (SPA):**
1. `GET /id/oauth/login?returnUrl=%2F` — SPA login page
2. `GET /oauth-ssr/allow-redirect` — CORS/redirect validation
3. `GET /id/oauth/redirect?returnPath=...` — redirects to login.seek.com
4. `GET login.seek.com/authorize?client_id=8OVhpvtaI9n5...` — OAuth authorize
5. `GET login.seek.com/login/callback?state=...` — login callback
6. `GET login.seek.com/authorize/resume?state=...` — resume authorize
7. `POST login.seek.com/oauth/token` — exchange code for token
8. `GET /id/oauth/callback?code=JVRB1qb...` — receives authorization code

---

## §4 Cross-References

- **OAuth flow details:** [AUTH-GUIDE.md §1](AUTH-GUIDE.md#§1-oauth-flow-diagram)
- **Token exchange endpoint:** [Vol4 §3.2](API-REFERENCE-vol4.md#§32-post-oauthtoken)
- **Token lifecycle:** [AUTH-GUIDE.md §3](AUTH-GUIDE.md#§3-token-lifecycle)
- **Session refresh:** [AUTH-GUIDE.md §4](AUTH-GUIDE.md#§4-session-refresh-strategy)
- **GraphQL endpoint (cookie auth):** [Vol2 §2](API-REFERENCE-vol2.md#§2-authentication)
- **Service architecture:** [SERVICE-MAP.md §2](SERVICE-MAP.md#§2-service-interaction-diagram)
