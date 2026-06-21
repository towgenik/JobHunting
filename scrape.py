# Invoked as: python scrape.py <url>  →  prints {"title", "description"} JSON on stdout.
# Launches its OWN headless browser (Scrapling) seeded with cookies harvested from the
# KasmVNC session. It never drives the VNC browser — that is slow and inconsistent (§2.4).
# ponytail: Phase 1 — DynamicFetcher + hardcoded jobstreet selectors. If anti-bot bites,
# swap to StealthyFetcher (bypasses Cloudflare out of the box). For Phase 2 site churn, use
# adaptive=True / auto_save so Scrapling relocates selectors when pages change.
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
    # JobStreet job-detail selectors. title first; fall back to h1 if the
    # data-automation attribute is renamed.
    title = (page.css_first('[data-automation="job-detail-title"]::text')
             or page.css_first('h1::text') or "")
    desc  = (page.css_first('[data-automation="jobDescriptionText"]')
             or page.css_first('[data-automation="jobDescription"]'))
    return {"title": str(title).strip(),
            "description": desc.text.strip() if desc else ""}


if __name__ == "__main__":
    print(json.dumps(asyncio.run(scrape(sys.argv[1]))))
