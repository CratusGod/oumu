#!/usr/bin/env python3
"""Headless UI suite for Oumu on Linux (ported from tests/*_headless.ps1).

Every window renders, scripted taps/drags/typing change what they should (checked in Rune's
element tree), and no text sticks out of its container. Nothing opens on screen and nothing
touches the mouse or keyboard; settings and data go to a fresh folder per run.

Usage: headless.py    (CRATUS_PROFILE=fast|release picks the build)
"""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PROFILE = os.environ.get("CRATUS_PROFILE", "debug")
RUN_TIMEOUT = 90  # a hung run is killed and fails; it never stalls the suite
X, A = "×", "→"


class Suite:
    def __init__(self, app: str) -> None:
        self.exe = ROOT / "target" / PROFILE / app
        self.out = ROOT / "target" / f"{app}-headless-linux"
        shutil.rmtree(self.out, ignore_errors=True)
        self.out.mkdir(parents=True)
        self.env = dict(os.environ)
        self.passed = 0
        self.failed: list[str] = []

    def check(self, name: str, ok: bool, detail: str = "") -> None:
        if ok:
            self.passed += 1
            print(f"  ok   {name} {detail}")
        else:
            self.failed.append(f"{name} {detail}")
            print(f"  FAIL {name} {detail}")

    def run(self, args: list[str]) -> tuple[int, str]:
        try:
            p = subprocess.run([str(self.exe), *args], env=self.env, capture_output=True, text=True, timeout=RUN_TIMEOUT)
        except subprocess.TimeoutExpired:
            print(f"  (killed after {RUN_TIMEOUT} s: {' '.join(args)})")
            return 124, ""
        (self.out / "stderr.txt").write_text(p.stderr)
        return p.returncode, p.stdout

    def listed(self) -> list[str]:
        _, out = self.run(["--list-windows"])
        return [l.strip() for l in out.splitlines() if l.strip() and not l.startswith("rune:")]

    def renders(self, windows: list[str], at: float, min_bytes) -> None:
        names = self.listed()
        for w in windows:
            if w not in names:
                self.check(f"listed: {w}", False)
                continue
            png = self.out / f"{w}.png"
            code, _ = self.run(["--window", w, "--snap", str(png), "--at", str(at)])
            size = png.stat().st_size if png.exists() else 0
            self.check(f"renders: {w}", code == 0 and size > min_bytes(w), f"{size:,} bytes")

    def script(self, window: str, name: str, events: list[dict], at: float, expect: list[str], absent: list[str] = ()) -> None:
        sp = self.out / f"{name}.json"
        sp.write_text(json.dumps({"events": events}))
        tree = self.out / f"{name}.tree.json"
        code, _ = self.run(["--window", window, "--script", str(sp), "--tree", str(tree), "--at", str(at)])
        # case-insensitive, like PowerShell's -like in the Windows suites
        text = tree_text(tree).casefold()
        missing = [e for e in expect if e.casefold() not in text]
        present = [e for e in absent if e.casefold() in text]
        over = overflows(tree)
        detail = ("missing: " + " | ".join(missing)) if missing else ("unexpected: " + " | ".join(present)) if present else ("text cut off: " + " | ".join(over)) if over else ("exit " + str(code) if code else "")
        self.check(f"{window} / {name}", code == 0 and not missing and not present and not over, detail)

    def done(self, label: str) -> int:
        print(f"\n{label}: {self.passed} passed, {len(self.failed)} failed")
        return len(self.failed)


def tree_text(path: Path) -> str:
    if not path.exists():
        return ""
    raw = path.read_text(encoding="utf-8")
    vals = re.findall(r'"(?:name|text)":\s*"((?:[^"\\]|\\.)*)"', raw)
    return "\n".join(json.loads(f'"{v}"') for v in vals)


def overflows(path: Path) -> list[str]:
    """Text that sticks out past its container on the right (cut off or over a neighbour)."""
    if not path.exists():
        return []
    root = json.loads(path.read_text(encoding="utf-8")).get("accessibility")
    found: list[str] = []

    def walk(n: dict, right: float) -> None:
        b = n.get("bounds")
        if b:
            r = float(b[0]) + float(b[2])
            name = n.get("name") or ""
            is_text = n.get("role") in ("label", "text", "statictext")
            badge = re.fullmatch(r"[0-9]+\+?", name) is not None  # count badges sit over a corner
            if is_text and name and not badge and b[2] > 0 and r > right + 1.5:
                found.append(f"{name[:40]} (+{r - right:.0f}px)")
            acts = n.get("actions") or []
            if not ("scroll-left" in acts or "scroll-right" in acts):
                right = min(right, r)
        for c in n.get("children") or []:
            if c:
                walk(c, right)

    if root:
        walk(root, float("inf"))
    return found


def tap(key: str, t: float) -> dict:
    return {"t": t, "type": "tap", "key": key}


def oumu() -> int:
    s = Suite("oumu")
    data = s.out / "data"
    data.mkdir()
    s.env["OUMU_DATA_DIR"] = str(data)
    panels = ["review", "review-empty", "chat", "chat-thread", "plagiarism", "tone", "performance", "paraphraser", "paraphraser-result", "humanizer", "reactions", "translate", "explain", "rewriter", "detector", "detector-loading", "history", "onboarding", "settings"]
    surfaces = ["tab", "tab-open", "button", "button-linting", "card", "card-long", "card-mute"]
    print("== windows render")
    s.renders(panels + surfaces, 1.0, lambda w: 50000 if w in panels else 1500)
    print("== nothing cut off")
    for w in panels + surfaces:
        s.script(w, f"layout-{w}", [], 1.0, [])
    print("== review")
    s.script("review", "first-open", [], 0.5, ["Why?", "Review suggestions", "Correctness", "Spelling", "library", "liberty", "Accept", "Dismiss", "Skip this app"])
    s.script("review", "clarity-tab", [tap("cat/clarity", 0.2)], 0.6, ["Wordy phrase", "I think"], ["tomorow"])
    s.script("review", "open-second", [tap("alert/a2", 0.2)], 0.6, ["tomorrow", "tomorow"])
    s.script("review-empty", "no-field", [], 0.5, ["Click into a text field", "Explorer"])
    s.script("review", "rail-chat", [tap("rail/chat", 0.2)], 0.6, ["Oumu AI Chat", "Summarize this page", "Review 7 suggestions"])
    s.script("review", "rail-settings", [tap("rail/settings", 0.2)], 0.6, ["Your AI key", "Appearance", "Skipped apps"])
    print("== tools")
    s.script("chat", "type", [tap("chat-input", 0.2), {"t": 0.3, "type": "type", "text": "Hello Oumu"}], 1.2, ["Hello Oumu"])
    s.script("chat-thread", "thread", [], 0.5, ["Summarize this page.", "library tomorrow"])
    s.script("humanizer", "voices", [], 0.5, ["Create a voice", "The Naturalist", "The Scholar"])
    s.script("humanizer", "create", [tap("create-voice", 0.2)], 0.6, ["Writing sample", "Save voice"])
    s.script("reactions", "audiences", [tap("audience", 0.2)], 0.6, ["My manager", "Investors", "Create audience"])
    s.script("paraphraser", "styles", [], 0.5, ["Change writing style", "Academic", "Shorten"])
    s.script("paraphraser-result", "result", [], 0.5, ["heading to the library", "Replace text", "Copy", "Show changes"])
    s.script("paraphraser-result", "diff", [tap("diff", 0.2)], 0.6, ["Hide changes"])
    s.script("rewriter", "ready", [], 0.5, ["AI Rewriter is ready to go", "Scan my text"])
    # the scan runs on a real-time worker while headless time is virtual: a fast (release)
    # build reaches 2 s before the worker is done, so these wait longer than on Windows
    s.script("review", "rewriter-auto-scan", [tap("rail/rewriter", 0.3)], 4.0, ["AI phrases reviewed", "it is important to note that", "delve into"], ["Scan my text"])
    s.script("review", "rewriter-dismiss", [tap("rail/rewriter", 0.3), tap("dismiss", 3.5)], 4.2, ["1/2"])
    s.script("humanizer", "voice-form", [tap("create-voice", 0.2)], 0.8, ["e.g. The Storyteller", "Paste something you wrote"])
    s.script("humanizer", "voice-needs-name", [tap("create-voice", 0.2), tap("save-voice", 0.5)], 1.0, ["Give the voice a name first."])
    s.script("detector", "hook", [], 0.5, ["Detect AI generated text", "Check this text"])
    s.script("detector-loading", "steps", [], 0.5, ["Scanning your text"])
    s.script("plagiarism", "hook", [], 0.5, ["Check for plagiarism", "APA"])
    s.script("translate", "ready", [], 0.5, ["Ready to translate.", "Translate all", "Auto-detect", "English (American)"])
    s.script("explain", "ready", [], 0.5, ["Spot something confusing?", "Explain the whole page"])
    s.script("tone", "no-ai", [tap("detect", 0.2)], 2.5, ["Set up AI in Settings"])
    s.script("performance", "score", [], 0.5, ["72", "Overall score", "Readability (Flesch)", "3 issues"])
    s.script("onboarding", "choices", [], 0.5, ["Choose how Oumu thinks", "Your AI key", "Local model (optional)", "Skip for now"])
    print("== settings")
    s.script("settings", "placeholders", [], 0.5, ["AQ.", "Optional, e.g. http://localhost:8888"])
    s.script("settings", "advanced", [tap("advanced", 0.2)], 0.6, ["Base URL", "Test connection"])
    s.script("settings", "key-kind", [tap("key", 0.2), {"t": 0.3, "type": "type", "text": "sk-ant-x"}], 1.2, ["Anthropic key"])
    s.script("settings", "name", [], 0.5, ["Name (used for the avatar and greetings)", "Dock side", "Icon rail"])
    print("== overlays")
    s.script("card", "spelling", [], 0.5, ["Correctness", "library", "Accept", "Dismiss", "Add to dictionary"])
    s.script("card-mute", "offer", [], 0.5, ["Turn it off", "dismissed"])
    s.script("card-long", "diff", [], 0.5, ["I think", "Accept"], ["Add to dictionary"])
    s.script("tab-open", "tab", [], 0.5, ["Open Oumu"])
    s.script("button", "button", [], 0.5, ["Open Oumu"])
    return s.done("oumu headless")


if __name__ == "__main__":
    which = sys.argv[1] if len(sys.argv) > 1 else "all"
    failed = 0
    if which in ("oumu", "all"):
        failed += oumu()
    sys.exit(1 if failed else 0)
