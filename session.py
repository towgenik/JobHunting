# Harvests jobstreet.com login cookies from the KasmVNC Chrome (over CDP) → session.json.
# NOTE: jobstreet.co.id redirects to id.jobstreet.com; cookies live under .jobstreet.com
# and id.jobstreet.com. SESSION_HOST default is "jobstreet.com" to catch both.
# This is the ONLY use of the container's CDP port; scraping never drives the VNC browser.
# ponytail: connect_over_cdp — WebSocket to existing Chrome, no browser launch needed
import json, os
from pathlib import Path
from playwright.sync_api import sync_playwright

CDP_URL = os.environ.get("CDP_URL", "http://localhost:9223")
# jobstreet.co.id redirects to id.jobstreet.com; cookies live under .jobstreet.com
# and id.jobstreet.com. Use "jobstreet.com" to harvest both via domain substring match.
HOST    = os.environ.get("SESSION_HOST", "jobstreet.com")
OUT     = Path(os.environ.get("SESSION_FILE",
             str(Path.home() / ".local" / "share" / "job-agent" / "session.json")))

OUT.parent.mkdir(parents=True, exist_ok=True)
with sync_playwright() as p:
    browser = p.chromium.connect_over_cdp(CDP_URL)   # attach to the running KasmVNC Chrome
    ctx = browser.contexts[0]                         # default context holds the login
    cookies = [c for c in ctx.cookies(f"https://{HOST}") if HOST in c["domain"]]
    json.dump(cookies, OUT.open("w"))
print(f"wrote {len(cookies)} cookies → {OUT}")
