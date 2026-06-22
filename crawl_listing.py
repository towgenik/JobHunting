# Invoked as: python crawl_listing.py <url>  →  prints JSON array of detail URLs on stdout.
# Paginates through JobStreet search results, discovers individual job-detail URLs, and
# feeds them into the Rust pipeline via stdout. Uses the same cookies + fetcher as scrape.py.
#
# Selectors (probed 2026-06-22 against id.jobstreet.com/id/software-jobs):
#   job cards:   article[data-card-type="JobCard"]  → data-job-id attribute
#   title links: a[href*="origin=cardTitle"]        → extract /id/job/<id>
#   next page:   a[aria-label="Selanjutnya"]         → href with ?page=N
#   Results per page: 30 (standard JobStreet pagination)
#
# ponytail: uses DynamicFetcher like scrape.py. If anti-bot bites with rapid page loads,
# swap to StealthyFetcher (bypasses Cloudflare out of the box) and add inter-page delays.
import os, sys, json, asyncio, re
from pathlib import Path
from scrapling.fetchers import DynamicFetcher

SESSION_FILE = Path(os.environ.get("SESSION_FILE",
    str(Path.home() / ".local" / "share" / "job-agent" / "session.json")))

MAX_PAGES = int(os.environ.get("CRAWL_MAX_PAGES", "3"))  # safety cap; override for deeper crawls


def load_cookies():
    if not SESSION_FILE.exists():
        sys.exit(f"no session at {SESSION_FILE} — run `python session.py` after logging in via noVNC")
    return json.loads(SESSION_FILE.read_text())


async def crawl(listing_url: str) -> list[str]:
    """Paginate through listing_url, extract unique detail URLs, return as list."""
    cookies = load_cookies()
    seen: set[str] = set()
    detail_urls: list[str] = []
    page_url = listing_url
    page_num = 0

    while page_url and page_num < MAX_PAGES:
        page_num += 1
        print(f"[crawl_listing] page {page_num}: {page_url}", file=sys.stderr)

        page = await DynamicFetcher.async_fetch(
            page_url, cookies=cookies, network_idle=True, headless=True
        )

        # Extract detail URLs from job cards via the card-title links.
        # Using origin=cardTitle avoids wrapper/footer links that have empty text.
        card_title_links = page.css('a[href*="origin=cardTitle"]')
        new_on_page = 0
        for link in card_title_links:
            href = link.attrib.get('href', '')
            m = re.match(r'(/id/job/\d+)', href)
            if m:
                full_url = 'https://id.jobstreet.com' + m.group(1)
                if full_url not in seen:
                    seen.add(full_url)
                    detail_urls.append(full_url)
                    new_on_page += 1

        print(f"[crawl_listing]   found {new_on_page} new URLs ({len(detail_urls)} total)",
              file=sys.stderr)

        # Follow the "Selanjutnya" (Next) link if present
        next_links = page.css('a[aria-label="Selanjutnya"]')
        if next_links:
            next_href = next_links[0].attrib.get('href', '')
            if next_href:
                # Resolve relative URL
                from urllib.parse import urljoin
                page_url = urljoin(page_url, next_href)
            else:
                page_url = None
        else:
            page_url = None  # no more pages

    print(f"[crawl_listing] done: {len(detail_urls)} URLs across {page_num} pages",
          file=sys.stderr)
    return detail_urls


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit("usage: python crawl_listing.py <listing-url>")
    urls = asyncio.run(crawl(sys.argv[1]))
    print(json.dumps(urls))
