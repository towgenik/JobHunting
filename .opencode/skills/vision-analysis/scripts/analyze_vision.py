#!/usr/bin/env python3
"""analyze_vision.py — send images to any OpenAI-compatible vision API.

Reads ~/.agents/vision-analysis.json (override with --config). Schema:

  {
    "groups": {
      "<group-name>": {
        "base_url": "https://...",                required
        "api_key":  "...",                        required
        "models": {
          "<model-id>": {
            "context_length": 65536,              required (drives token budget)
            "max_output":     4096,               optional (max_tokens)
            "max_dim":        4096                optional (hard pixel cap)
          }
        }
      },
      ...
    },
    "default_model":   "<model-id>",              required
    "fallback_models": ["<model-id>", ...],       optional
    "fallback_on":     ["429", "5xx", "network"], optional (default shown)
    "quality":         85,                        optional (JPEG quality)
    "request_timeout": 120                        optional (HTTP timeout seconds)
  }

A "group" is the unit of (base_url, api_key). Many models can share one group.
Each model carries its own context window and output limit. There is no
hardcoded base URL list and no provider/family dispatch — every endpoint
and scaling parameter comes from the config.

Back-compat: VISION_API_KEY / VISION_BASE_URL / VISION_MODEL env vars still
work (with a stderr warning) when the config file is absent.
"""

from __future__ import annotations

import argparse
import base64
import io
import json
import math
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path

try:
    from PIL import Image
except ImportError:
    sys.stderr.write("error: Pillow is required. Install: pip install Pillow\n")
    sys.exit(2)

SKILL_DIR = Path(__file__).resolve().parent.parent
DEFAULT_CONFIG_PATH = Path.home() / ".agents" / "vision-analysis.json"

DEFAULT_DISABLE_PAYLOAD = {
    "reasoning": {"enabled": False},
    "chat_template_kwargs": {"enable_thinking": False},
}


# ---------- config ----------

class APIError(RuntimeError):
    pass


def load_json(path: Path) -> dict:
    if not path.exists():
        return {}
    try:
        return json.loads(path.read_text())
    except json.JSONDecodeError as e:
        sys.stderr.write(f"error: {path}: invalid JSON: {e}\n")
        sys.exit(2)


def check_perms(path: Path) -> None:
    """Warn if the config file is world/group readable (it has API keys)."""
    if not path.exists():
        return
    mode = path.stat().st_mode & 0o777
    if mode & 0o077:
        sys.stderr.write(
            f"warning: {path} is mode {mode:o}; expected 0600 (has API keys). "
            f"Fix with: chmod 600 {path}\n"
        )


def load_config(path: Path) -> dict:
    """Load config. Back-compat: synthesize from VISION_* env vars if absent."""
    check_perms(path)
    config = load_json(path)
    if config:
        return config

    # Back-compat: VISION_* env vars when no config file exists.
    if all(os.environ.get(k) for k in ("VISION_API_KEY", "VISION_BASE_URL")):
        sys.stderr.write(
            f"warning: VISION_* env vars deprecated; move credentials to {path} "
            f"(JSON, chmod 600).\n"
        )
        config = {
            "groups": {"env": {
                "base_url": os.environ["VISION_BASE_URL"],
                "api_key":  os.environ["VISION_API_KEY"],
                "models": {},
            }},
        }
        if os.environ.get("VISION_MODEL"):
            sys.stderr.write(
                f"warning: VISION_MODEL env var deprecated; set 'default_model' "
                f"in {path} or use --model.\n"
            )
            config["default_model"] = os.environ["VISION_MODEL"]
        return config

    return {}


def find_model(model_id: str, config: dict) -> tuple[str, str, str, dict]:
    """Locate a model in any group. Returns (group_name, api_key, base_url, meta).

    Lookup order:
      1. Exact match in some group's 'models' dict — use that group's credentials.
      2. Single-group config with empty/missing models dict (env-var back-compat
         or fresh setup) — use that group with default meta.
      3. Otherwise: error.
    """
    groups = config.get("groups", {})
    for name, group in groups.items():
        if name.startswith("_") or not isinstance(group, dict):
            continue
        models = group.get("models", {})
        if model_id in models:
            meta = models[model_id] or {}
            api_key = meta.get("api_key") or group.get("api_key")
            base_url = meta.get("base_url") or group.get("base_url")
            if not (api_key and base_url):
                raise APIError(
                    f"group '{name}' for model '{model_id}' is missing api_key "
                    f"or base_url")
            return name, api_key, base_url, meta

    if len(groups) == 1:
        name, group = next(iter(groups.items()))
        api_key = group.get("api_key")
        base_url = group.get("base_url")
        if api_key and base_url:
            return name, api_key, base_url, {}

    raise APIError(
        f"model '{model_id}' not found in any group's 'models' dict. Register "
        f"it under the appropriate group, or pass --api-key and --base-url.")


# ---------- image scaling (single function, no per-family dispatch) ----------

def resize_for_budget(img: Image.Image, max_dim: int | None,
                      budget: int, num_images: int) -> Image.Image:
    """Resize one image to fit a visual-token budget, with an optional hard
    pixel cap. Generic for all vision APIs — uses 48px patch estimation.

    Pipeline: fit-to-budget → cap-to-max_dim.
    """
    w, h = img.size

    # Step 1: scale to fit visual-token budget (48px patch estimation).
    target_tokens = max(196, (budget // max(num_images, 1)) - 128)
    current_tokens = (w / 48) * (h / 48)
    if current_tokens > target_tokens:
        scale = math.sqrt(target_tokens / current_tokens) * 0.98
        w, h = int(w * scale), int(h * scale)

    # Step 2: hard pixel cap (for APIs with explicit dimension limits).
    if max_dim and max(w, h) > max_dim:
        cap_scale = max_dim / float(max(w, h))
        w, h = int(w * cap_scale), int(h * cap_scale)

    if (w, h) == img.size:
        return img
    return img.resize((w, h), Image.Resampling.LANCZOS)


def compress_image(path: str, max_dim: int | None, budget: int,
                   num_images: int, quality: int) -> tuple[str, str]:
    with Image.open(path) as src:
        src.load()
        img = src if src.mode not in ("RGBA", "P") else src.convert("RGB")
        img = resize_for_budget(img, max_dim, budget, num_images)
        sys.stderr.write(
            f"  {Path(path).name}: {src.size[0]}x{src.size[1]} -> "
            f"{img.size[0]}x{img.size[1]}"
            + (f" (max_dim={max_dim}, " if max_dim else " (")
            + f"budget={budget}, q={quality})\n"
        )
        buf = io.BytesIO()
        img.save(buf, format="JPEG", quality=quality, optimize=True)
    return base64.b64encode(buf.getvalue()).decode("ascii"), "image/jpeg"


# ---------- API call ----------

def call_api(model_id: str, question: str, images: list[tuple[str, str]],
             api_key: str, base_url: str, max_output: int | None,
             timeout: int, thinking_enabled: bool = False,
             disable_payload: dict | None = None) -> dict:
    content = [{"type": "text", "text": question}]
    for b64, mime in images:
        content.append({
            "type": "image_url",
            "image_url": {"url": f"data:{mime};base64,{b64}"},
        })
    payload: dict = {
        "model": model_id,
        "messages": [{"role": "user", "content": content}],
        "stream": False,
    }
    if max_output:
        payload["max_tokens"] = max_output

    # Vision = perception. Thinking mode default-off to avoid timeouts / empty outputs.
    # Per-model `thinking: true` overrides. disable_payload comes from config.
    if not thinking_enabled:
        payload.update(disable_payload or DEFAULT_DISABLE_PAYLOAD)

    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}",
    }
    if "openrouter" in base_url.lower():
        headers["HTTP-Referer"] = "https://github.com/pi-agent"

    url = f"{base_url.rstrip('/')}/chat/completions"
    req = urllib.request.Request(url, data=json.dumps(payload).encode("utf-8"),
                                 headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        raise APIError(
            f"HTTP {e.code} from {url}: "
            f"{e.read().decode('utf-8', 'replace')[:500]}") from None
    except urllib.error.URLError as e:
        raise APIError(f"network error: {e.reason}") from None


def should_fallback(err_msg: str, fallback_on: list[str]) -> bool:
    msg = err_msg.lower()
    return any(tok in msg for tok in (s.lower() for s in fallback_on))


# ---------- main ----------

def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="analyze_vision.py",
        description="Send images to any OpenAI-compatible vision model.",
        epilog=f"Config file: {DEFAULT_CONFIG_PATH}",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("question", help="Prompt text for the model")
    p.add_argument("images", nargs="+", help="One or more image paths")
    p.add_argument("--model", help="Override default_model from config")
    p.add_argument("--api-key", help="Override resolved api_key for the model's group")
    p.add_argument("--base-url", help="Override resolved base_url for the model's group")
    p.add_argument("--max-output", type=int,
                   help="Override max_output tokens (default: model meta or provider default)")
    p.add_argument("--quality", type=int,
                   help="JPEG quality 1-100 (default: config or 85)")
    p.add_argument("--timeout", type=int,
                   help="HTTP timeout seconds (default: config or 120)")
    p.add_argument("--no-fallback", action="store_true",
                   help="Disable fallback chain; fail on first error")
    p.add_argument("--config", type=Path, default=DEFAULT_CONFIG_PATH,
                   help=f"Config JSON (default: {DEFAULT_CONFIG_PATH})")
    return p


def resolve_image_path(p_str: str) -> Path | None:
    path = Path(p_str)
    if path.exists():
        return path
    # Back-compat: try relative to project root (3 levels up from script).
    alt = SKILL_DIR.parent.parent.parent / p_str
    return alt if alt.exists() else None


def main() -> int:
    args = build_parser().parse_args()
    config_path: Path = args.config
    config = load_config(config_path)

    image_paths: list[Path] = []
    for p_str in args.images:
        path = resolve_image_path(p_str)
        if path is None:
            sys.stderr.write(f"error: image not found: {p_str}\n")
            return 1
        image_paths.append(path)

    primary = args.model or config.get("default_model")
    if not primary:
        sys.stderr.write(
            f"error: no model selected. Set 'default_model' in {config_path} "
            f"or pass --model.\n")
        return 2
    fallback_on = config.get("fallback_on", ["429", "timeout", "5xx", "network"])
    fallback_chain = [] if args.no_fallback else list(config.get("fallback_models", []))
    model_chain = [primary] + fallback_chain

    quality = args.quality if args.quality is not None else config.get("quality", 85)
    timeout = args.timeout if args.timeout is not None else config.get("request_timeout", 120)

    thinking_cfg = config.get("thinking") or {}
    default_thinking = bool(thinking_cfg.get("enabled", False))
    disable_payload = thinking_cfg.get("disable_payload") or DEFAULT_DISABLE_PAYLOAD

    last_error = ""
    for idx, model_id in enumerate(model_chain):
        try:
            group_name, api_key, base_url, meta = find_model(model_id, config)
        except APIError as e:
            last_error = str(e)
            sys.stderr.write(f"[{model_id}] {last_error}\n")
            if idx < len(model_chain) - 1:
                continue
            return 2

        if args.api_key:
            api_key = args.api_key
        if args.base_url:
            base_url = args.base_url

        budget = int(meta.get("context_length", 4096) * 0.8)
        max_dim = meta.get("max_dim")
        max_output = args.max_output or meta.get("max_output")
        thinking_enabled = bool(meta.get("thinking", default_thinking))

        sys.stderr.write(f"[{model_id}] group={group_name} base_url={base_url} "
                         f"budget={budget} tok thinking={'on' if thinking_enabled else 'off'}\n")
        try:
            images_data = [compress_image(str(p), max_dim, budget,
                                          len(image_paths), quality)
                           for p in image_paths]
        except Exception as e:
            last_error = f"compression failed: {e}"
            sys.stderr.write(f"[{model_id}] {last_error}\n")
            continue

        try:
            result = call_api(model_id, args.question, images_data,
                              api_key, base_url, max_output, timeout,
                              thinking_enabled, disable_payload)
        except APIError as e:
            last_error = str(e)
            sys.stderr.write(f"[{model_id}] failed: {last_error}\n")
            if idx < len(model_chain) - 1 and should_fallback(last_error, fallback_on):
                sys.stderr.write(f"[{model_id}] falling through to next model\n")
                continue
            return 1

        usage = result.get("usage", {})
        print("--- Usage ---")
        print(f"Input: {usage.get('prompt_tokens', '?')} tokens | "
              f"Output: {usage.get('completion_tokens', '?')} tokens | "
              f"Total: {usage.get('total_tokens', '?')} tokens")
        print("--- Result ---")
        try:
            print(result["choices"][0]["message"]["content"])
        except (KeyError, IndexError):
            sys.stderr.write(f"error: unexpected response shape: {result}\n")
            return 1
        return 0

    sys.stderr.write(f"error: all models in fallback chain failed. "
                     f"Last error: {last_error}\n")
    return 1


if __name__ == "__main__":
    sys.exit(main())
