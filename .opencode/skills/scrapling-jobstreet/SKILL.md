---
name: scrapling-jobstreet
description: Two gotchas hit during M2 — jobstreet domain redirect and scrapling css_first() not existing
---

# Scrapling + JobStreet — Two Gotchas

## Gotcha 1: jobstreet.co.id domain redirect

**Problem:** `session.py` with `SESSION_HOST=jobstreet.co.id` wrote 0 cookies even after the user logged in.

**Root cause:** `jobstreet.co.id` permanently redirects to `id.jobstreet.com`. The browser logs in at `id.jobstreet.com`, so cookies are set on `.jobstreet.com` (parent domain) and `id.jobstreet.com`. The filter `HOST in c["domain"]` with HOST=`jobstreet.co.id` matches neither.

**Fix:** Set `SESSION_HOST=jobstreet.com` (the shared parent domain). The substring filter then catches both `.jobstreet.com` and `id.jobstreet.com` cookies. This is the default in session.py now.

**ponytail:** If jobstreet ever moves to a different apex domain, update SESSION_HOST. The `.co.id` TLD is effectively dead — don't use it in cookie harvesting or URL construction.

---

## Gotcha 2: scrapling 0.4.x has no `css_first()` method

**Problem:** Architecture §4 spec used `page.css_first('[data-automation="..."]::text')` — this raises `AttributeError: 'Response' object has no attribute 'css_first'`.

**Root cause:** The `css_first()` method does not exist in scrapling 0.4.x. The spec was written against an assumed API that was never real.

**Fix:** Use `page.css(selector)` which returns a `Selectors` list, then index `[0]` for the first match:
```python
els = page.css('[data-automation="job-detail-title"]')
title = str(els[0].text).strip() if els else ""
```
For full child text: `els[0].get_all_text(separator='\n', strip=True)`.
The `::text` pseudo-selector (Scrapy-style) is also not supported — use `.text` (direct text node) or `.get_all_text()` (all descendants).

**ponytail:** `DynamicFetcher.async_fetch()` returns a `Response(Selector)`. Available methods: `css()`, `find()`, `find_all()`, `xpath()`. None of the Scrapy shorthands (`css_first`, `extract`, `extract_first` on a single selector) exist at the top-level Response.

---

## Gotcha 3: description selector is `jobAdDetails`, not `jobDescriptionText`/`jobDescription`

**Problem:** Architecture spec described `[data-automation="jobDescriptionText"]` with `[data-automation="jobDescription"]` as fallback. Neither exists on live `id.jobstreet.com` job-detail pages.

**Root cause:** The spec was written speculatively; actual page inspection shows the description lives in `[data-automation="jobAdDetails"]`.

**Fix:** Use `page.css('[data-automation="jobAdDetails"]')` for the description. Verified across 3 job detail pages (2026-06-21).

**ponytail:** When selectors fail silently (empty string returned), always probe with a full `data-automation` dump before tuning:
```python
auto_vals = set(el.attrib.get('data-automation','') for el in page.css('[data-automation]'))
print(sorted(auto_vals))
```
