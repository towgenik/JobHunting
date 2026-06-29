#!/usr/bin/env python3
"""Fast JobStreet job detail scraper — pure HTTP, no browser needed.

Replaces scrape.py (headless browser). Fetches the SSR-rendered HTML page
and extracts title + description from data-automation attributes.

Usage: python3 scrape_api.py <url>  →  {"title", "description"} on stdout
"""
import sys, json, re, urllib.request

HEADERS = {
    "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36",
    "Accept": "text/html,application/xhtml+xml",
    "Accept-Language": "id-ID,id;q=0.9,en;q=0.8",
}


def scrape(url: str) -> dict:
    """Fetch job detail page and extract title + description + company."""
    req = urllib.request.Request(url, headers=HEADERS)
    with urllib.request.urlopen(req, timeout=15) as resp:
        html = resp.read().decode("utf-8", errors="replace")

    # Title: try data-automation first, fall back to <title> tag
    title = ""
    m = re.search(
        r'data-automation="job-detail-title"[^>]*>(.*?)</', html, re.DOTALL
    )
    if m:
        title = re.sub(r"<[^>]+>", "", m.group(1)).strip()
    if not title:
        m = re.search(r"<title>(.*?)\s*-.*?</title>", html)
        if m:
            title = m.group(1).strip()

    # Company: data-automation="advertiser-name"
    company = ""
    m = re.search(
        r'data-automation="advertiser-name"[^>]*>(.*?)</', html, re.DOTALL
    )
    if m:
        company = re.sub(r"<[^>]+>", "", m.group(1)).strip()

    # Description: capture from jobAdDetails to the report/footer boundary.
    # Previously we only captured the jobAdDetails container (758 chars).
    # The page has additional sections (company profile, employer questions,
    # etc.) between jobAdDetails and the report-job-ad section. Capture them all.
    description = ""
    m = re.search(r'<div[^>]*data-automation="jobAdDetails"', html)
    if m:
        content_start = m.start()
        after = html[content_start:]
        # Stop at report-job-ad-toggle or <footer — whichever comes first.
        end = after.find('data-automation="report-job-ad-toggle"')
        if end < 0:
            end = after.find("<footer")
        if end > 0:
            raw = after[:end]
            text = re.sub(r"<br\s*/?>", "\n", raw)
            text = re.sub(r"<[^>]+>", "", text)
            text = re.sub(r"\n{3,}", "\n\n", text)
            description = text.strip()

    return {"title": title, "description": description, "company": company}


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit("usage: python3 scrape_api.py <url>")
    result = scrape(sys.argv[1])
    print(json.dumps(result, ensure_ascii=False))
