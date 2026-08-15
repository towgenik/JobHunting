# API Reference Vol 2 — GraphQL API

**Endpoint:** `https://id.jobstreet.com/graphql`
**Method:** POST
**Last updated:** 2026-07-08
**Total unique operations:** 47

---

## Table of Contents

- [§1 Overview](#§1-overview)
- [§2 Authentication](#§2-authentication)
  - [§2.1 Public Operations (7)](#§21-public-operations-7)
  - [§2.2 Authenticated Operations (28+)](#§22-authenticated-operations-28)
- [§3 Operations](#§3-operations)
  - [§3.1 Search Operations](#§31-search-operations)
  - [§3.2 Profile Read Operations](#§32-profile-read-operations)
  - [§3.3 Profile Mutations](#§33-profile-mutations)
  - [§3.4 Utility Operations](#§34-utility-operations)
- [§4 Request Format](#§4-request-format)
- [§5 Response Schema](#§5-response-schema)
- [§6 Error Codes](#§6-error-codes)
- [§7 Cross-References](#§7-cross-references)

---

## §1 Overview

GraphQL API serving the id.jobstreet.com frontend. Single endpoint, POST-only.
Operations discovered via mitmproxy capture sessions (176 entries, 47 unique operations).

**Capture sessions:**
- `all-sessions.jsonl` — 50 entries, 35 unique ops
- `profile-settings.jsonl` — 37 entries, 32 unique ops
- `search-entry.jsonl` — 8 entries, 8 unique ops
- `search-sort.jsonl` — 22 entries, 5 unique ops
- `search-filters.jsonl` — 28 entries, 7 unique ops
- `search-pagination.jsonl` — 10 entries, 5 unique ops
- `logged-out-browse.jsonl` — 3 entries, 3 unique ops
- `existing.jsonl` — 18 entries, 13 unique ops

**Related:** Full query strings captured in [GRAPHQL-REFERENCE.md](GRAPHQL-REFERENCE.md).

---

## §2 Authentication

Two tiers: public (no auth) and authenticated (cookies required).

### §2.1 Public Operations (7)

These work without any cookies or auth headers:

| Operation | Status | Notes |
|-----------|--------|-------|
| `GetBanner` | OK | Returns banner template data |
| `getKeywordSuggestions` | OK | Keyword suggestions (empty with test data) |
| `getClassificationOptions` | OK | Job classifications with subcategories |
| `GetProfileVisibilityOptions` | OK | Visibility setting labels |
| `GetWorkTypeOptions` | OK | Work type options (Full time, Part time, etc.) |
| `GetSupportedCountries` | OK | Country list (AU, GB, ID, etc.) |
| `JobCountsV6` | UNSTABLE_QUERY_ERROR | Query structure issue, not auth-related |

### §2.2 Authenticated Operations (28+)

All viewer-based queries require valid session cookies. Without valid cookies:
`UNAUTHENTICATED` error.

**Required cookies:** Session cookies from authenticated browser. Critical auth cookies:
- `__cf_bm` (Cloudflare bot management) — ~30 min expiry
- `_cfuvid` (Cloudflare visitor ID) — session-only
- `auth0.*.is.authenticated` — time-limited JWT
- `_legacy_auth0.*` — auth0 session token

**Cookie harvest:** Use `session.py` with CDP to extract from running browser.
See [Scrape SKILL §session.py](../.claude/skills/scrape/SKILL.md).

---

## §3 Operations

### §3.1 Search Operations

6 operations for job search, counts, and autocomplete.

#### §3.1.1 JobSearchV6

| Field | Value |
|-------|-------|
| **Count** | 14 captured invocations |
| **Variables** | `params`, `locale`, `timezone` |
| **Auth** | Unknown (not tested without auth) |
| **Sources** | search-entry, search-sort, search-filters, search-pagination |

Primary job search operation. Returns job listings with full metadata.
Variant of the public REST API ([Vol1 §3.1](API-REFERENCE-vol1.md#§31-get-search--job-search))
but through GraphQL.

#### §3.1.2 JobCountsV6

| Field | Value |
|-------|-------|
| **Count** | 22 captured invocations |
| **Variables** | `params` |
| **Auth** | Public (but returns UNSTABLE_QUERY_ERROR in testing) |
| **Sources** | all-sessions, logged-out-browse, search-entry/sort/filters/pagination, existing |

Returns job counts by classification. Used for facet counts in search UI.

**Known issue:** Returns `UNSTABLE_QUERY_ERROR` regardless of auth status.
May require specific feature flags or query stability tokens.

#### §3.1.3 JobCountV7

| Field | Value |
|-------|-------|
| **Count** | 14 captured invocations |
| **Variables** | `params` |
| **Auth** | Unknown |
| **Sources** | search-entry, search-sort, search-filters, search-pagination |

Newer version of job count operation.

#### §3.1.4 SearchSavedAndAppliedJobs

| Field | Value |
|-------|-------|
| **Count** | 11 captured invocations |
| **Variables** | `jobIds` |
| **Auth** | Likely authenticated |
| **Sources** | search-entry, search-sort, search-filters, search-pagination |

Checks saved/applied status for given job IDs. Requires viewer context.

#### §3.1.5 getKeywordSuggestions

| Field | Value |
|-------|-------|
| **Count** | 6 captured invocations |
| **Variables** | `country`, `keyword`, `visitorId`, `count` |
| **Auth** | **Public** |
| **Sources** | all-sessions, logged-out-browse, search-entry |

Keyword autocomplete. Returns suggestions for search input.

#### §3.1.6 searchLocationsSuggest

| Field | Value |
|-------|-------|
| **Count** | 7 captured invocations |
| **Variables** | `query`, `recentLocation`, `count`, `locale`, `country`, `visitorId`, `isRemoteEnabled` |
| **Auth** | Unknown |
| **Sources** | search-entry, search-filters |

Location autocomplete for search.

---

### §3.2 Profile Read Operations

27 operations for reading user profile data. All require authentication.

#### §3.2.1 Identity Operations

| Operation | Variables | Key Fields |
|-----------|-----------|------------|
| `GetId` | — | `viewer._id` |
| `getUserInfo` | — | `viewer.id`, `_id`, `emailAddress`, `personalDetails` |
| `GetUserDetails` | — | Full profile details |
| `GetPersonalDetails` | `countryCode`, `languageCode`, `zone` | Name, phone, location |
| `getCandidateId` | — | Candidate identifier |
| `getCandidateStatus` | — | Candidate account status |
| `getProvisioningStatus` | — | Account provisioning state |

#### §3.2.2 Profile Core

| Operation | Variables | Key Fields |
|-----------|-----------|------------|
| `GetProfile` | `zone`, `languageCode`, `locale` | Full profile data |
| `GetProfileAvatar` | — | Avatar image URL |
| `GetProfileInsights` | `zone`, `locale`, `timezone` | Profile analytics |
| `GetProfileVisibility2` | `locale` | Visibility setting |
| `GetProfileVisibilityOptions` | `locale`, `zone` | Visibility labels (**public**) |
| `GetPublicProfile` | `zone`, `locale`, `platform` | Public profile URL |

#### §3.2.3 Skills & Qualifications

| Operation | Variables | Key Fields |
|-----------|-----------|------------|
| `GetSkills` | — | Skill keywords |
| `GetSuggestedSkills` | `languageCode`, `zone` | AI-suggested skills |
| `GetConfirmedQualifications` | `languageCode`, `zone` | Verified education |
| `GetUnconfirmedQualifications` | `languageCode`, `zone` | Unverified education |
| `GetConfirmedRoles` | — | Verified work history |
| `GetUnconfirmedRoles` | — | Unverified work history |
| `GetLicences` | `languageCode`, `zone` | Professional licenses |
| `GetLanguageProficiencies` | — | Language skills |
| `getReferenceChecks` | `locale` | Reference info |
| `getVerifications` | `zone`, `locale` | Identity verifications |

#### §3.2.4 Career & Preferences

| Operation | Variables | Key Fields |
|-----------|-----------|------------|
| `GetCareerObjectives` | — | Personal statement |
| `GetPreferredClassification` | — | Job category preferences |
| `GetPreferredLocations2` | `zone`, `languageCode` | Location preferences |
| `GetSalaryPreferences` | `languageCode`, `countryCode` | Salary expectations |
| `GetWorkTypes` | — | Preferred work types |
| `GetWorkTypeOptions` | `languageCode` | Work type labels (**public**) |
| `GetAvailability` | `languageCode` | Job search status |
| `GetApproachability` | — | Recruiter visibility |
| `GetRightsToWork` | `languageCode`, `zone` | Work authorization |
| `GetResumes` | `languageCode` | Resume files |
| `GetScore` | `zone`, `languageCode` | Profile completion score |
| `GetSupportedCountries` | `languageCode` | Country list (**public**) |
| `getClassificationOptions` | `zone`, `languageCode` | Job category taxonomy (**public**) |

---

### §3.3 Profile Mutations

3 mutation operations.

#### §3.3.1 UpdateProfile

| Field | Value |
|-------|-------|
| **Count** | 1 captured invocation |
| **Variables** | `zone`, `languageCode`, `personalDetailsData`, `currentLocation2Data`, `profileVisibilityData` |
| **Auth** | Required |

Updates personal details, location, and visibility settings.

#### §3.3.2 UpdatePreferredClassification

| Field | Value |
|-------|-------|
| **Count** | 1 captured invocation |
| **Variables** | `classificationData` |
| **Auth** | Required |

Updates career classification preferences.

#### §3.3.3 sendLoginCallbackEvent

| Field | Value |
|-------|-------|
| **Count** | 1 captured invocation |
| **Variables** | — |
| **Auth** | Required |

Post-login event callback.

---

### §3.4 Utility Operations

2 utility operations.

#### §3.4.1 FeatureFlags

| Field | Value |
|-------|-------|
| **Count** | 4 captured invocations |
| **Variables** | `flags`, `visitorContext`, `deviceContext`, `experienceContext`, `applicationContext` |
| **Auth** | Unknown |
| **Sources** | search-entry, search-filters, existing |

Feature flag evaluation. Returns enabled/disabled state for frontend features.

#### §3.4.2 GetBanner

| Field | Value |
|-------|-------|
| **Count** | 13 captured invocations |
| **Variables** | `placement`, `country`, `locale`, `zone`, `roleId`, `visitorId`, `candidateId`, `loggedIn`, `keywords` |
| **Auth** | **Public** |
| **Sources** | All capture sessions |

Promotional banner content. Used for footer/hero banners on search and detail pages.

---

## §4 Request Format

### §4.1 HTTP Request Structure

```
POST https://id.jobstreet.com/graphql
Content-Type: application/json
Cookie: <session cookies for authenticated ops>

{
  "operationName": "OperationName",
  "query": "query OperationName($var: Type) { ... }",
  "variables": { "var": "value" }
}
```

### §4.2 Minimal Example (Public)

```bash
curl -X POST https://id.jobstreet.com/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "operationName": "GetBanner",
    "query": "query GetBanner($placement: String!) { banner(placement: $placement) { ... } }",
    "variables": {"placement": "search-footer"}
  }'
```

### §4.3 Authenticated Example

```bash
curl -X POST https://id.jobstreet.com/graphql \
  -H "Content-Type: application/json" \
  -H "Cookie: __cf_bm=...; _cfuvid=...; auth0.*.is.authenticated=..." \
  -d '{
    "operationName": "GetId",
    "query": "query GetId { viewer { _id __typename } }",
    "variables": {}
  }'
```

### §4.4 Schema Validation

The endpoint validates queries strictly:
- Unknown types/fields → `GRAPHQL_VALIDATION_FAILED`
- Invalid operation structure → 400 error
- This can be used for schema discovery

---

## §5 Response Schema

### §5.1 Standard Response

```json
{
  "data": { ... },
  "errors": [ ... ],
  "extensions": { ... }
}
```

### §5.2 Error Response (Unauthenticated)

```json
{
  "data": null,
  "errors": [
    {
      "message": "UNAUTHENTICATED",
      "locations": [...],
      "path": ["viewer"],
      "extensions": { "code": "UNAUTHENTICATED" }
    }
  ]
}
```

### §5.3 Error Response (Validation Failed)

```json
{
  "errors": [
    {
      "message": "Cannot query field \"unknown\" on type \"Query\".",
      "extensions": { "code": "GRAPHQL_VALIDATION_FAILED" }
    }
  ]
}
```

---

## §6 Error Codes

| Error Code | Cause | Fix |
|------------|-------|-----|
| `UNAUTHENTICATED` | Missing or expired session cookies | Re-authenticate via browser login |
| `UNSTABLE_QUERY_ERROR` | Query stability/feature flag issue | May need specific query shape or feature flag |
| `GRAPHQL_VALIDATION_FAILED` | Unknown fields/types in query | Check schema, fix query |

---

## §7 Cross-References

- **Public REST alternative for search:** [Vol1 §3.1](API-REFERENCE-vol1.md#§31-get-search--job-search)
- **Full query strings:** [GRAPHQL-REFERENCE.md §1](GRAPHQL-REFERENCE.md#§1-all-operations)
- **Fragment definitions:** [GRAPHQL-REFERENCE.md §2](GRAPHQL-REFERENCE.md#§2-fragment-definitions)
- **Variables schema:** [GRAPHQL-REFERENCE.md §3](GRAPHQL-REFERENCE.md#§3-variables-schema-per-operation)
- **Auth flow:** [AUTH-GUIDE.md](AUTH-GUIDE.md)
- **Cookie details:** [AUTH-GUIDE.md §2](AUTH-GUIDE.md#§2-cookie-structure)
- **Internal endpoints (OAuth SSR):** [Vol3 §3.3](API-REFERENCE-vol3.md#§33-oauth-ssr-endpoints)
- **Service architecture:** [SERVICE-MAP.md §2](SERVICE-MAP.md#§2-service-interaction-diagram)
