# Invoked as: python scrape.py <url>  →  prints {"title", "description"} JSON on stdout.
# Launches its OWN headless browser (Scrapling) seeded with cookies harvested from the
# KasmVNC session. It never drives the VNC browser — that is slow and inconsistent (§2.4).
# ponytail: Phase 1 — DynamicFetcher + hardcoded jobstreet selectors. If anti-bot bites,
# swap to StealthyFetcher (bypasses Cloudflare out of the box). For Phase 2 site churn, use
# adaptive=True / auto_save so Scrapling relocates selectors when pages change.
#
# Selector notes (probed 2026-06-21 against id.jobstreet.com):
#   title:       [data-automation="job-detail-title"] → .text (direct text node)
#   title fallback: h1 → .get_all_text()
#   description: [data-automation="jobAdDetails"]    → .get_all_text()
#   NOTE: jobDescriptionText / jobDescription do NOT exist on id.jobstreet.com pages.
#         The actual description container is jobAdDetails (Architecture §4 updated).
import os, sys, json, asyncio
from pathlib import Path
from scrapling.fetchers import DynamicFetcher

SESSION_FILE = Path(os.environ.get("SESSION_FILE",
    str(Path.home() / ".local" / "share" / "job-agent" / "session.json")))


def load_cookies():
    if not SESSION_FILE.exists():
        sys.exit(f"no session at {SESSION_FILE} — run `python session.py` after logging in via noVNC (§2.4)")
    return json.loads(SESSION_FILE.read_text())


async def scrape(url: str) -> dict:
    # Own headless browser; the harvested cookies carry the logged-in session.
    page = await DynamicFetcher.async_fetch(url, cookies=load_cookies(),
                                            network_idle=True, headless=True)

    # Title: [data-automation="job-detail-title"] .text; fall back to h1
    # Scrapling's css() returns a Selectors list — index [0] for first element.
    title_els = page.css('[data-automation="job-detail-title"]')
    if title_els:
        title = str(title_els[0].text).strip()
    else:
        h1_els = page.css('h1')
        title = str(h1_els[0].get_all_text()).strip() if h1_els else ""

    # Description: [data-automation="jobAdDetails"] (NOT jobDescriptionText/jobDescription
    # — those selectors don't exist on id.jobstreet.com job-detail pages).
    desc_els = page.css('[data-automation="jobAdDetails"]')
    description = str(desc_els[0].get_all_text(separator='\n', strip=True)).strip() if desc_els else ""

    return {"title": title, "description": description}


if __name__ == "__main__":
    print(json.dumps(asyncio.run(scrape(sys.argv[1]))))
