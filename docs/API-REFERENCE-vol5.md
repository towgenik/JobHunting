# API Reference Vol 5 — Other Services

**Last updated:** 2026-07-08

---

## Table of Contents

- [§1 Overview](#§1-overview)
- [§2 Services](#§2-services)
  - [§2.1 Metrics Forwarder — seek-metrics-forwarder](#§21-metrics-forwarder--seek-metrics-forwarder)
  - [§2.2 Image CDN — image-service-cdn](#§22-image-cdn--image-service-cdn)
  - [§2.3 Branding Gateway — bx-branding-gateway](#§23-branding-gateway--bx-branding-gateway)
  - [§2.4 Tracking — tracking.engineering](#§24-tracking--trackingengineering)
  - [§2.5 Google Analytics / Ads](#§25-google-analytics--ads)
  - [§2.6 Microsoft / Bing](#§26-microsoft--bing)
  - [§2.7 Facebook Pixel](#§27-facebook-pixel)
  - [§2.8 Segment Analytics](#§28-segment-analytics)
  - [§2.9 Hotjar](#§29-hotjar)
  - [§2.10 Tealium Tag Manager](#§210-tealium-tag-manager)
  - [§2.11 Qualtrics](#§211-qualtrics)
  - [§2.12 Other Tracking](#§212-other-tracking)
- [§3 Cross-References](#§3-cross-references)

---

## §1 Overview

SEEK/JobStreet integrates with numerous first-party and third-party services for
analytics, CDN, branding, and tracking. This volume documents all discovered services.

**Total unique domains:** 40
**Total unique endpoint patterns:** 221

Most of these are not useful for scraping — documented here for completeness and
to identify potential blocking/interference points.

---

## §2 Services

### §2.1 Metrics Forwarder — seek-metrics-forwarder

| Field | Value |
|-------|-------|
| **Domain** | `seek-metrics-forwarder.cloud.seek.com.au` |
| **Endpoint** | `POST /v1/send` |
| **Auth** | None |
| **Response** | HTTP 204 (No Content) |
| **Content-Type** | `application/json` |
| **Body** | `{"events": []}` |

Accepts empty event arrays. Used for telemetry/metrics from frontend.
**Not useful for scraping.**

### §2.2 Image CDN — image-service-cdn

| Field | Value |
|-------|-------|
| **Domain** | `image-service-cdn.seek.com.au` |
| **Pattern** | `GET /{hash}/{hash}` |
| **Auth** | None (public CDN) |
| **Response** | Image binary (PNG/JPG) or 404 |

Serves company logo images. URLs appear in job listing `branding` field.
8 unique image URLs observed in captures.

**Usage in job data:** Logo URLs from this CDN appear in the `branding` field
of search results. [See Vol1 §4.3.4](API-REFERENCE-vol1.md#§434-branding-object).

### §2.3 Branding Gateway — bx-branding-gateway

| Field | Value |
|-------|-------|
| **Domain** | `bx-branding-gateway.cloud.seek.com.au` |
| **Pattern** | `GET /{uuid}/jdpLogo` |
| **Auth** | Required |
| **Response** | `"Unauthorized: no auth has been configured"` |

Branding configuration service. Requires auth. 4 unique UUID-based URLs observed.
**Not accessible without credentials.**

### §2.4 Tracking — tracking.engineering

| Field | Value |
|-------|-------|
| **Domain** | `tracking.engineering.cloud.seek.com.au` |
| **Method** | GET |
| **Auth** | None |
| **Response** | HTTP 200 |

Analytics/tracking endpoint. **Not useful for scraping.**

### §2.5 Google Analytics / Ads

**29 endpoints** across Google's advertising ecosystem.

| Domain | Purpose | IDs |
|--------|---------|-----|
| `www.googletagmanager.com` | GTM tags (GA4, Google Ads, DoubleClick) | — |
| `analytics.google.com` | GA4 collect (POST) | `G-DSKCDC8253` |
| `www.google.com/ccm/collect` | CCM collect (10 calls) | — |
| `www.google.com/rmkt/collect/938064972/` | Remarketing (8 calls) | `AW-938064972` |
| `googleads.g.doubleclick.net` | Conversion tracking | — |
| `cm.g.doubleclick.net` | Cookie sync pixels | — |
| `ad.doubleclick.net` | Activity tracking | — |
| `16129666.fls.doubleclick.net` | Floodlight activities | `DC-16129666` |
| `securepubads.g.doubleclick.net` | GPT ad scripts | — |

### §2.6 Microsoft / Bing

**12 endpoints** for Microsoft advertising and analytics.

| Domain | Purpose | ID |
|--------|---------|-----|
| `bat.bing.com` | UET tracking (actions + postbacks) | `187182209` |
| `scripts.clarity.ms` | Clarity session recording | — |
| `f.clarity.ms` | Clarity data collection | — |
| `www.clarity.ms` | UET tag | — |

### §2.7 Facebook Pixel

**12 endpoints** for Facebook advertising.

| Domain | Purpose | ID |
|--------|---------|-----|
| `connect.facebook.net` | FB pixel script + config | — |
| `www.facebook.com/tr/` | FB events (PageView, Search, ViewContent) | `1580784772211622` |

### §2.8 Segment Analytics

**3 endpoints** for Segment analytics platform.

| Domain | Purpose | ID |
|--------|---------|-----|
| `cdn.segment.com` | Analytics.js + project settings | — |
| `api.segment.io` | Track/identify events | `DvY7kWpoiUVpDNHDqjU` |

### §2.9 Hotjar

**2 endpoints** for Hotjar session recording.

| Domain | Purpose | ID |
|--------|---------|-----|
| `static.hotjar.com` | Hotjar script | — |
| `script.hotjar.com` | Hotjar modules | `640499` |

### §2.10 Tealium Tag Manager

**13 endpoints** for Tealium tag management.

| Domain | Purpose |
|--------|---------|
| `tags.tiqcdn.com` | Tag manager scripts (12 unique tags) |

### §2.11 Qualtrics

**4 endpoints** for Qualtrics survey intercepts.

| Domain | Purpose |
|--------|---------|
| `siteintercept.qualtrics.com` | Survey intercepts |
| `zn0oyh8mqq82zt2ws-seek.siteintercept.qualtrics.com` | Seek-specific instance |

### §2.12 Other Tracking

| Domain | Purpose |
|--------|---------|
| `lcto.aips-sol.com` | AIPs event tracking |
| `cdn.branch.io` | Branch deep linking SDK |
| `www.datadoghq-browser-agent.com` | Datadog RUM |

### §2.13 Ad-Tech Cookie Syncs

~30 ad-tech partners for cookie syncing:
- Doubleclick ecosystem
- StackAdapt
- Taboola
- OpenX
- Yieldmo
- Others

**Implication for scraping:** These third-party scripts may interfere with
headless browser operations. Consider blocking analytics/ad domains in
scraper to reduce noise and improve performance.

---

## §3 Cross-References

- **All domains list:** [SERVICE-MAP.md §1](SERVICE-MAP.md#§1-domains-and-roles)
- **Service architecture:** [SERVICE-MAP.md §2](SERVICE-MAP.md#§2-service-interaction-diagram)
- **CDN structure:** [SERVICE-MAP.md §3](SERVICE-MAP.md#§3-cdn-structure)
- **Analytics pipeline:** [SERVICE-MAP.md §4](SERVICE-MAP.md#§4-analytics-pipeline)
- **Third-party integrations:** [SERVICE-MAP.md §5](SERVICE-MAP.md#§5-third-party-integrations)
