# Service Map — SEEK/JobStreet Platform Architecture

**Last updated:** 2026-07-08
**Total unique domains:** 40
**Total unique endpoint patterns:** 221

---

## Table of Contents

- [§1 Domains and Roles](#§1-domains-and-roles)
- [§2 Service Interaction Diagram](#§2-service-interaction-diagram)
- [§3 CDN Structure](#§3-cdn-structure)
- [§4 Analytics Pipeline](#§4-analytics-pipeline)
- [§5 Third-Party Integrations](#§5-third-party-integrations)

---

## §1 Domains and Roles

### §1.1 Primary API Domains

| Domain | Role | Auth | API Reference |
|--------|------|------|---------------|
| `jobsearch-api.cloud.seek.com.au` | Public REST API | None | [Vol1](API-REFERENCE-vol1.md) |
| `id.jobstreet.com` | GraphQL + SPA + OAuth SSR | Cookies / Bearer | [Vol2](API-REFERENCE-vol2.md), [Vol3](API-REFERENCE-vol3.md) |
| `login.seek.com` | OAuth auth server | Varies | [Vol4](API-REFERENCE-vol4.md) |

### §1.2 First-Party Service Domains

| Domain | Role | Auth |
|--------|------|------|
| `seek-metrics-forwarder.cloud.seek.com.au` | Telemetry/metrics | None |
| `image-service-cdn.seek.com.au` | Company logo CDN | None |
| `bx-branding-gateway.cloud.seek.com.au` | Branding config | Required |
| `tracking.engineering.cloud.seek.com.au` | Analytics tracking | None |

### §1.3 Static Asset Domains

| Domain | Role |
|--------|------|
| `id.jobstreet.com/static/ca-search-ui/` | Search UI JS bundles |
| `id.jobstreet.com/static/profile/` | Profile page JS bundles |
| `id.jobstreet.com/static/settings/` | Settings page JS bundles |

### §1.4 Third-Party Analytics Domains

| Domain | Role | ID |
|--------|------|-----|
| `www.googletagmanager.com` | Tag manager | — |
| `analytics.google.com` | GA4 collect | `G-DSKCDC8253` |
| `www.google.com` | Remarketing, CCM | `AW-938064972` |
| `bat.bing.com` | Bing UET | `187182209` |
| `scripts.clarity.ms` | Clarity recording | — |
| `connect.facebook.net` | FB pixel | `1580784772211622` |
| `cdn.segment.com` | Segment analytics | `DvY7kWpoiUVpDNHDqjU` |
| `static.hotjar.com` | Hotjar sessions | `640499` |
| `tags.tiqcdn.com` | Tealium tags | — |
| `siteintercept.qualtrics.com` | Surveys | — |
| `www.datadoghq-browser-agent.com` | Datadog RUM | — |

### §1.5 Ad-Tech Domains

| Domain | Role |
|--------|------|
| `googleads.g.doubleclick.net` | Google Ads conversion |
| `cm.g.doubleclick.net` | Cookie sync pixels |
| `ad.doubleclick.net` | Activity tracking |
| `16129666.fls.doubleclick.net` | Floodlight activities |
| `securepubads.g.doubleclick.net` | GPT ad scripts |
| `cdn.branch.io` | Deep linking |
| `lcto.aips-sol.com` | AIPs tracking |

---

## §2 Service Interaction Diagram

### §2.1 Request Flow (Browser)

```
┌─────────────────────────────────────────────────────────────────┐
│                        Browser (User)                           │
└───────────────────────────┬─────────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
        ▼                   ▼                   ▼
┌───────────────┐  ┌────────────────┐  ┌────────────────┐
│ id.jobstreet  │  │ jobsearch-api  │  │ login.seek.com │
│   .com        │  │ .cloud.seek    │  │                │
│               │  │  .com.au       │  │                │
│ ├─ /graphql   │  │ ├─ /v5/search  │  │ ├─ /authorize  │
│ ├─ /api/*     │  │                │  │ ├─ /oauth/token│
│ ├─ /oauth-ssr*│  │                │  │ ├─ /time       │
│ └─ /id/*      │  │                │  │ └─ /v2/logout  │
└───────┬───────┘  └────────────────┘  └────────────────┘
        │
        ├─── GraphQL (cookies) ──> Viewer queries, search
        ├─── /api/* (Bearer) ──> Search persist/unpersist
        └─── /oauth-ssr/* ──> OAuth flow redirects
```

### §2.2 Scraper Flow (JobHunting App)

```
┌─────────────────────────────────────────────────────────────┐
│                    JobHunting App                            │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  1. Cookie Harvest (session.py)                             │
│     └── CDP ──> KasmVNC Chrome ──> session.json             │
│                                                             │
│  2. Job Search (crawl_listing.py)                           │
│     └── HTML scraping OR                                   │
│         jobsearch-api.cloud.seek.com.au/v5/search           │
│         (public REST, no auth)                              │
│                                                             │
│  3. Job Detail (scrape.py)                                  │
│     └── HTML scraping with cookies                          │
│         OR GraphQL jobDetailsPersonalised (cookie auth)     │
│                                                             │
│  4. Profile Data (optional)                                 │
│     └── GraphQL viewer queries (cookie auth)                │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### §2.3 OAuth Flow (Auth Server)

```
┌──────────────┐     ┌──────────────┐     ┌───────────────┐
│ id.jobstreet │     │ login.seek   │     │  Auth0 / IdP  │
│   .com       │     │   .com       │     │               │
└──────┬───────┘     └──────┬───────┘     └───────┬───────┘
       │                    │                     │
       │  /oauth-ssr/login  │                     │
       │───────────────────>│                     │
       │                    │  /authorize         │
       │                    │────────────────────>│
       │                    │                     │
       │                    │  Login form         │
       │                    │<────────────────────│
       │                    │                     │
       │                    │  Credentials        │
       │                    │────────────────────>│
       │                    │                     │
       │                    │  Auth code          │
       │                    │<────────────────────│
       │                    │                     │
       │  /oauth-ssr/       │  POST /oauth/token  │
       │  callback          │────────────────────>│
       │<───────────────────│                     │
       │                    │  Access + Refresh   │
       │                    │<────────────────────│
       │  Session set       │                     │
       │<───────────────────│                     │
```

---

## §3 CDN Structure

### §3.1 Image CDN

**Domain:** `image-service-cdn.seek.com.au`
**Pattern:** `GET /{hash}/{hash}`
**Auth:** None (public)

Serves company logo images. URLs appear in job listing `branding` field.

Example from captured data:
```
https://image-service-cdn.seek.com.au/{hash1}/{hash2}
```

### §3.2 Static Assets

**Domain:** `id.jobstreet.com`
**Paths:**
- `/static/ca-search-ui/houston/*.js` — Search UI bundles (8 files)
- `/static/profile/*.js` — Profile page bundles (6 files)
- `/static/settings/*.js` — Settings page bundles (4 files)

### §3.3 Branding Gateway

**Domain:** `bx-branding-gateway.cloud.seek.com.au`
**Pattern:** `GET /{uuid}/jdpLogo`
**Auth:** Required

Branding configuration for employer logos. 4 unique UUID-based URLs observed.
Returns "Unauthorized: no auth has been configured" without credentials.

---

## §4 Analytics Pipeline

### §4.1 Analytics Stack

```
┌─────────────────────────────────────────────────────────┐
│                    Browser Events                        │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐              │
│  │ Tealium  │  │ Segment  │  │ DataDog  │              │
│  │ (tags)   │  │ (events) │  │ (RUM)    │              │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘              │
│       │              │              │                    │
│       ▼              ▼              ▼                    │
│  ┌──────────────────────────────────────┐               │
│  │         Google Tag Manager           │               │
│  └──────────────────┬───────────────────┘               │
│                     │                                   │
│       ┌─────────────┼─────────────┐                     │
│       ▼             ▼             ▼                     │
│  ┌─────────┐  ┌──────────┐  ┌──────────┐               │
│  │  GA4    │  │ Google   │  │ Facebook │               │
│  │         │  │  Ads     │  │  Pixel   │               │
│  └─────────┘  └──────────┘  └──────────┘               │
│                                                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐              │
│  │ Hotjar  │  │ Clarity  │  │Qualtrics │              │
│  │(sessions)│  │(sessions)│  │(surveys) │              │
│  └──────────┘  └──────────┘  └──────────┘              │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### §4.2 Event Flow

1. User actions trigger events in browser
2. Tealium/Segment capture and route events
3. GTM distributes to Google properties (GA4, Ads, DoubleClick)
4. Hotjar/Clarity record sessions
5. Datadog RUM monitors performance
6. seek-metrics-forwarder collects custom metrics

### §4.3 Key Analytics IDs

| Service | ID | Purpose |
|---------|-----|---------|
| Google Analytics | `G-DSKCDC8253` | GA4 property |
| Google Ads | `AW-938064972` | Conversion tracking |
| DoubleClick | `DC-16129666` | Floodlight activities |
| Bing UET | `187182209` | Bing ads tracking |
| Facebook Pixel | `1580784772211622` | FB conversion |
| Hotjar | `640499` | Session recording |
| Segment | `DvY7kWpoiUVpDNHDqjU` | Event routing |

---

## §5 Third-Party Integrations

### §5.1 Advertising Platforms

| Platform | Domains | Purpose |
|----------|---------|---------|
| Google Ads | `googleads.g.doubleclick.net`, `www.google.com/rmkt/collect/*` | Remarketing, conversion |
| Doubleclick | `cm.g.doubleclick.net`, `ad.doubleclick.net`, `16129666.fls.doubleclick.net` | Cookie sync, floodlight |
| Facebook | `connect.facebook.net`, `www.facebook.com/tr/` | Pixel tracking, conversion |
| Bing | `bat.bing.com` | UET tracking |
| StackAdapt | Various | Programmatic ads |
| Taboola | Various | Content ads |
| OpenX | Various | Ad exchange |
| Yieldmo | Various | Ad format |

### §5.2 Analytics Platforms

| Platform | Domains | Purpose |
|----------|---------|---------|
| Google Analytics | `analytics.google.com`, `www.googletagmanager.com` | Web analytics |
| Segment | `cdn.segment.com`, `api.segment.io` | Event routing |
| Hotjar | `static.hotjar.com`, `script.hotjar.com` | Session recording |
| Microsoft Clarity | `scripts.clarity.ms`, `f.clarity.ms` | Session recording |
| Datadog | `www.datadoghq-browser-agent.com` | RUM monitoring |
| Tealium | `tags.tiqcdn.com` | Tag management |
| Qualtrics | `siteintercept.qualtrics.com` | Survey intercepts |

### §5.3 Infrastructure Partners

| Partner | Domains | Purpose |
|---------|---------|---------|
| Cloudflare | `__cf_bm`, `_cfuvid` cookies | Bot management, CDN |
| Auth0 | `auth0.*` cookies | Authentication |
| Branch | `cdn.branch.io` | Deep linking |
| AIPs | `lcto.aips-sol.com` | Event tracking |

### §5.4 Implications for Scraping

**Potential interference points:**
- Cloudflare bot management may challenge automated requests
- Analytics scripts may detect headless browsers
- Cookie syncs generate additional network traffic

**Recommendations:**
- Block analytics/ad domains in headless browser to reduce noise
- Use `StealthyFetcher` if Cloudflare blocks `DynamicFetcher`
- Monitor for Cloudflare challenges in response headers
- Keep session cookies fresh to avoid auth issues

---

## Cross-References

- **Public API:** [Vol1](API-REFERENCE-vol1.md)
- **GraphQL API:** [Vol2](API-REFERENCE-vol2.md)
- **Internal endpoints:** [Vol3](API-REFERENCE-vol3.md)
- **Auth endpoints:** [Vol4](API-REFERENCE-vol4.md)
- **Other services:** [Vol5](API-REFERENCE-vol5.md)
- **Auth guide:** [AUTH-GUIDE.md](AUTH-GUIDE.md)
- **Scrape pipeline:** [Scrape SKILL](../.claude/skills/scrape/SKILL.md)
