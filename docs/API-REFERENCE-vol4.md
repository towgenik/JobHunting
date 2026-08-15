# API Reference Vol 4 — Auth Endpoints

**Auth server:** `https://login.seek.com`
**Last updated:** 2026-07-08
**OAuth client_id:** `8OVhpvtaI9n5` (from captured traffic)

---

## Table of Contents

- [§1 Overview](#§1-overview)
- [§2 OAuth Flow](#§2-oauth-flow)
- [§3 Endpoints](#§3-endpoints)
  - [§3.1 GET /time — Time Sync](#§31-get-time--time-sync)
  - [§3.2 POST /oauth/token — Token Exchange](#§32-post-oauthtoken--token-exchange)
  - [§3.3 GET /authorize — Authorization Endpoint](#§33-get-authorize--authorization-endpoint)
  - [§3.4 Passwordless Flow](#§34-passwordless-flow)
  - [§3.5 GET /v2/logout — Logout](#§35-get-v2logout--logout)
- [§4 Token Lifecycle](#§4-token-lifecycle)
- [§5 Cross-References](#§5-cross-references)

---

## §1 Overview

SEEK's authentication server at `login.seek.com`. Handles OAuth 2.0 authorization
code flow for all SEEK properties (JobStreet, SEEK, etc.). Issues JWTs for API access.

**Key IDs from captured traffic:**
- OAuth client_id: `8OVhpvtaI9n5`
- Auth server: `login.seek.com`
- Token endpoint: `POST login.seek.com/oauth/token`
- Grant type: Authorization code

---

## §2 OAuth Flow

Authorization Code flow with PKCE (likely). Sequence from captured traffic:

```
1. GET  /id/oauth/login?returnUrl=%2F           → SPA login page
2. GET  /oauth-ssr/allow-redirect                → CORS validation
3. GET  /id/oauth/redirect?returnPath=...        → redirect to login.seek.com
4. GET  login.seek.com/authorize?client_id=...   → OAuth authorize
5. GET  login.seek.com/login/callback?state=...  → login callback
6. GET  login.seek.com/authorize/resume?state=... → resume authorize
7. POST login.seek.com/oauth/token               → exchange code for token
8. GET  /id/oauth/callback?code=JVRB1qb...       → SPA receives code
```

**Detailed flow diagram:** [AUTH-GUIDE.md §1](AUTH-GUIDE.md#§1-oauth-flow-diagram)

---

## §3 Endpoints

### §3.1 GET /time — Time Sync

| Field | Value |
|-------|-------|
| **Full URL** | `https://login.seek.com/time` |
| **Method** | GET |
| **Auth** | None |
| **Response** | `{"unixtime": 1783529620066}` |

Returns server unix timestamp in milliseconds. Called many times during session
for time synchronization (NTP-like). Used by the frontend for token expiry
calculations.

**No auth required.** Public endpoint.

### §3.2 POST /oauth/token — Token Exchange

| Field | Value |
|-------|-------|
| **Full URL** | `https://login.seek.com/oauth/token` |
| **Method** | POST |
| **Auth** | Authorization code (from /authorize flow) |
| **Content-Type** | `application/x-www-form-urlencoded` or `application/json` |
| **GET response** | 404 (POST-only, as expected) |

Exchanges authorization code for access/refresh tokens. Called during OAuth callback.

**Expected parameters (standard OAuth 2.0):**
```
grant_type=authorization_code
code=<authorization_code>
redirect_uri=<callback_url>
client_id=8OVhpvtaI9n5
```

**Response:** JSON with access_token, refresh_token, token_type, expires_in.
See [AUTH-GUIDE.md §3](AUTH-GUIDE.md#§3-token-lifecycle) for token details.

### §3.3 GET /authorize — Authorization Endpoint

| Field | Value |
|-------|-------|
| **Full URL** | `https://login.seek.com/authorize` |
| **Method** | GET |
| **Auth** | User interaction (login form) |
| **Response** | 302 redirect |

OAuth authorization endpoint. Redirects user to login form, then back with
authorization code.

**Query parameters (observed):**
- `client_id` — OAuth client ID (`8OVhpvtaI9n5`)
- `state` — CSRF protection state parameter
- `redirect_uri` — Callback URL
- Other standard OAuth params (response_type, scope, etc.)

### §3.4 Passwordless Flow

Observed endpoint in captured traffic:

| Endpoint | Method | Notes |
|----------|--------|-------|
| `/login/callback?state=...` | GET | Login callback with state |
| `/authorize/resume?state=...` | GET | Resume authorize after login |

The login flow supports passwordless authentication (email link / OTP).
After initial authorize, user completes login, then flow resumes at
`/authorize/resume` to complete token exchange.

### §3.5 GET /v2/logout — Logout

| Field | Value |
|-------|-------|
| **Full URL** | `https://login.seek.com/v2/logout` |
| **Method** | GET |
| **Auth** | User session |
| **Response** | 302 redirect |

Logout endpoint. Parameters include `client_id` and `returnTo` for
post-logout redirect.

**Logout sequence:**
1. `GET /id/oauth/logout` — SPA logout page
2. `GET /oauth-ssr/logout` — SSR logout handler
3. `GET login.seek.com/v2/logout?client_id=...&returnTo=...` — auth server logout
4. `GET /oauth-ssr/post-logout-redirect` — final redirect

---

## §4 Token Lifecycle

### §4.1 Token Types

| Token | Storage | Lifetime | Refresh |
|-------|---------|----------|---------|
| Access token | In-memory (SPA) | Short-lived (~minutes) | Via refresh token |
| Refresh token | Cookie/secure storage | Longer-lived | Re-auth if expired |
| Session cookie | Browser cookie | ~30 min (`__cf_bm`) | Auto-refreshed |

### §4.2 Token Flow

1. User authenticates → authorization code issued
2. Code exchanged for access_token + refresh_token at `/oauth/token`
3. Access token used as `Authorization: Bearer <token>` for API calls
4. When access token expires → use refresh token to get new one
5. When refresh token expires → full re-authentication required

### §4.3 Critical Expiry Times

- `__cf_bm` (Cloudflare): ~30 minutes
- `_cfuvid` (Cloudflare): session-only
- `auth0.*.is.authenticated`: time-limited JWT
- `_legacy_auth0.*`: auth0 session token

**Full lifecycle details:** [AUTH-GUIDE.md §3](AUTH-GUIDE.md#§3-token-lifecycle)

---

## §5 Cross-References

- **Full OAuth flow diagram:** [AUTH-GUIDE.md §1](AUTH-GUIDE.md#§1-oauth-flow-diagram)
- **Cookie structure:** [AUTH-GUIDE.md §2](AUTH-GUIDE.md#§2-cookie-structure)
- **Session refresh:** [AUTH-GUIDE.md §4](AUTH-GUIDE.md#§4-session-refresh-strategy)
- **Internal OAuth SSR endpoints:** [Vol3 §3.3](API-REFERENCE-vol3.md#§33-oauth-ssr-endpoints)
- **SPA OAuth endpoints:** [Vol3 §3.4](API-REFERENCE-vol3.md#§34-oauth-spa-endpoints)
- **Bearer token for API:** [Vol3 §2.1](API-REFERENCE-vol3.md#§21-bearer-token-requirement)
- **Service architecture:** [SERVICE-MAP.md §1](SERVICE-MAP.md#§1-domains-and-roles)
