#!/usr/bin/env python3
"""Fast JobStreet indexer using the public SEEK REST API — no browser required.

Usage:
    python3 index_api.py "backend engineer" --pages 5 --page-size 30
    python3 index_api.py "devops" --pages 3 --output urls.json

Output: JSON array of job objects on stdout, compatible with crawl_listing.py format.
"""

import argparse, json, sys, time, urllib.parse, urllib.request

API_BASE = "https://jobsearch-api.cloud.seek.com.au/v5/search"
HEADERS = {
    "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36",
    "Accept": "application/json",
}


def search(keywords: str, page: int = 1, page_size: int = 30,
           site_key: str = "ID", locale: str = "id-ID",
           date_range: int | None = None, sort_mode: str | None = None) -> dict:
    """Call the SEEK public search API. Returns parsed JSON response."""
    params: dict = {}
    if keywords:
        params["keywords"] = keywords
    params.update({
        "siteKey": site_key,
        "page": page,
        "pageSize": page_size,
        "locale": locale,
    })
    if date_range is not None:
        params["dateRange"] = date_range
    if sort_mode is not None:
        params["sortMode"] = sort_mode
    url = f"{API_BASE}?{urllib.parse.urlencode(params)}"
    req = urllib.request.Request(url, headers=HEADERS)
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read())


def index(keywords: str, pages: int = 3, page_size: int = 30,
          site_key: str = "ID", locale: str = "id-ID",
          delay: float = 0.3, date_range: int | None = None,
          sort_mode: str | None = None) -> list[dict]:
    """Index jobs from the search API across multiple pages.

    Returns a list of job dicts, each with:
      id, title, teaser, companyName, employer, locations, listingDate,
      workTypes, salaryLabel, roleId, url
    """
    jobs: list[dict] = []
    total_count = None

    for page in range(1, pages + 1):
        data = search(keywords, page=page, page_size=page_size,
                      site_key=site_key, locale=locale,
                      date_range=date_range, sort_mode=sort_mode)

        if total_count is None:
            total_count = data.get("totalCount", 0)
            max_pages = min(pages, -(-total_count // page_size))  # ceil division
            print(f"[index_api] Query: \"{keywords}\" → {total_count} total jobs, "
                  f"fetching up to {max_pages} pages ({min(max_pages * page_size, total_count)} jobs)",
                  file=sys.stderr)

        items = data.get("data", [])
        if not items:
            print(f"[index_api] Page {page}: no results — stopping", file=sys.stderr)
            break

        for item in items:
            job_id = item.get("id", "")
            jobs.append({
                "id": job_id,
                "title": item.get("title", ""),
                "teaser": item.get("teaser", ""),
                "companyName": item.get("companyName", ""),
                "employerName": item.get("employer", {}).get("name", ""),
                "locations": [loc.get("label", "") for loc in item.get("locations", [])],
                "listingDate": item.get("listingDate", ""),
                "listingDateDisplay": item.get("listingDateDisplay", ""),
                "workTypes": item.get("workTypes", []),
                "salaryLabel": item.get("salaryLabel", ""),
                "roleId": item.get("roleId", ""),
                "url": f"https://id.jobstreet.com/id/job/{job_id}",
                "classifications": [
                    {
                        "category": c.get("classification", {}).get("description", ""),
                        "subcategory": c.get("subclassification", {}).get("description", ""),
                    }
                    for c in item.get("classifications", [])
                ],
            })

        print(f"[index_api] Page {page}/{max_pages}: {len(items)} jobs "
              f"({len(jobs)} accumulated)", file=sys.stderr)

        if page < max_pages and page < pages:
            time.sleep(delay)

    return jobs


def extract_keywords_from_url(url: str) -> str:
    """Extract search keywords from a JobStreet listing URL.
    e.g. 'https://id.jobstreet.com/id/backend-engineer-jobs' → 'backend engineer'
         'https://id.jobstreet.com/id/software-jobs?keywords=rust' → 'rust'
    """
    import re
    # Try ?keywords= query param first
    parsed = urllib.parse.urlparse(url)
    qs = urllib.parse.parse_qs(parsed.query)
    if 'keywords' in qs:
        return qs['keywords'][0]
    # Extract from path: last segment, strip -jobs, hyphen→space
    path_parts = [p for p in parsed.path.split('/') if p]
    if path_parts:
        last = path_parts[-1]
        last = re.sub(r'-jobs$', '', last)
        return last.replace('-', ' ')
    return ''


def main():
    parser = argparse.ArgumentParser(description="Fast JobStreet indexer via SEEK API")
    parser.add_argument("keywords", nargs='?', default='',
                        help="Search keywords (e.g. 'backend engineer')")
    parser.add_argument("--url", help="Extract keywords from a JobStreet listing URL")
    parser.add_argument("--pages", type=int, default=3, help="Max pages to fetch (default: 3)")
    parser.add_argument("--page-size", type=int, default=30, help="Results per page (default: 30)")
    parser.add_argument("--site-key", default="ID", help="SEEK site key (default: ID)")
    parser.add_argument("--locale", default="id-ID", help="Locale (default: id-ID)")
    parser.add_argument("--delay", type=float, default=0.3, help="Delay between pages in seconds")
    parser.add_argument("--date-range", type=int, help="SEEK dateRange param (e.g. 1 = last 24h)")
    parser.add_argument("--sort", default=None, choices=["ListedDate", "KeywordRelevance"],
                        help="SEEK sortMode param")
    parser.add_argument("--output", "-o", help="Write to file instead of stdout")
    parser.add_argument("--urls-only", action="store_true", help="Output only URLs (compatible with crawl_listing.py)")
    args = parser.parse_args()

    keywords = args.keywords
    if args.url:
        keywords = extract_keywords_from_url(args.url)
        if not keywords:
            print(f"[index_api] ERROR: could not extract keywords from URL: {args.url}", file=sys.stderr)
            sys.exit(1)
        print(f"[index_api] URL → keywords: \"{keywords}\"", file=sys.stderr)

    jobs = index(keywords, pages=args.pages, page_size=args.page_size,
                 site_key=args.site_key, locale=args.locale, delay=args.delay,
                 date_range=args.date_range, sort_mode=args.sort)

    if args.urls_only:
        output = json.dumps([j["url"] for j in jobs])
    else:
        output = json.dumps(jobs, indent=2, ensure_ascii=False)

    if args.output:
        with open(args.output, "w") as f:
            f.write(output)
        print(f"[index_api] Wrote {len(jobs)} jobs to {args.output}", file=sys.stderr)
    else:
        print(output)

    print(f"[index_api] Done: {len(jobs)} jobs indexed", file=sys.stderr)


if __name__ == "__main__":
    main()
