"""Start the real Oumu for a Linux test run and talk to its debug probe.

Oumu's data, log and socket live in the run folder, so the user's own settings, history and
running Oumu are never touched. The user's AI settings may be copied in (never printed).
"""
from __future__ import annotations

import json
import os
import subprocess
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PROFILE = os.environ.get("CRATUS_PROFILE", "debug")
EXE = ROOT / "target" / PROFILE / "oumu"
PORT = 17999
REAL_SETTINGS = Path.home() / ".local/share/dev.cratusgod.oumu/settings.json"


def settings(data: Path, with_ai: bool) -> bool:
    """Write the run's settings; True when an AI provider was copied in."""
    s = {"onboarded": True, "show_button": True, "show_underlines": True, "target_language": "Vietnamese"}
    has_ai = False
    if with_ai and REAL_SETTINGS.exists():
        try:
            llm = json.loads(REAL_SETTINGS.read_text()).get("llm") or {}
        except (OSError, ValueError):
            llm = {}
        if llm.get("provider") not in (None, "", "none"):
            s["llm"] = llm
            has_ai = True
    if with_ai and os.environ.get("OUMU_E2E_KEY"):
        model = s.get("llm", {}).get("model", "") if s.get("llm", {}).get("provider") == "gemini" else ""
        s["llm"] = {"provider": "gemini", "base_url": "https://generativelanguage.googleapis.com/v1beta", "api_key": os.environ["OUMU_E2E_KEY"], "model": model}
        has_ai = True
    data.mkdir(parents=True, exist_ok=True)
    (data / "settings.json").write_text(json.dumps(s, indent=2))
    return has_ai


def env(data: Path) -> dict:
    e = dict(os.environ)
    e.update(OUMU_DATA_DIR=str(data), OUMU_SOCK=str(data / "oumu.sock"), OUMU_DEBUG_PORT=str(PORT), OUMU_E2E="1")
    e.setdefault("RUST_LOG", "info")
    return e


def start(data: Path, *args: str) -> subprocess.Popen:
    return subprocess.Popen([str(EXE), *args], env=env(data), stdout=subprocess.DEVNULL, stderr=open(data / "stderr.txt", "ab"))


def api(path: str, body: dict | None = None, timeout: float = 150):
    req = urllib.request.Request(f"http://127.0.0.1:{PORT}{path}", data=json.dumps(body or {}).encode(), headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode() or "null")


def wait(secs: float, cond, every: float = 0.25):
    end = time.time() + secs
    while time.time() < end:
        try:
            v = cond()
            if v:
                return v
        except Exception:
            pass
        time.sleep(every)
    return None


def scrub(s: str) -> str:
    """Never show a key: URLs and messages can carry one."""
    import re

    s = re.sub(r"(key=)[^&\"\s]+", r"\1***", s)
    s = re.sub(r"AIza[0-9A-Za-z_-]{10,}", "***", s)
    s = re.sub(r"AQ\.[0-9A-Za-z_.-]{10,}", "***", s)
    return re.sub(r"(sk-ant-|sk-|gsk_|sk-or-)[0-9A-Za-z_-]{8,}", "***", s)
