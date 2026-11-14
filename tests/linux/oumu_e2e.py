#!/usr/bin/env python3
"""Oumu E2E on Linux (Hyprland), ported from tests/oumu_e2e.ps1.

The real Oumu against a GTK3 test editor (its own process). Real pointer moves and clicks
(ydotool) open the suggestion card and press Accept; the debug probe (OUMU_DEBUG_PORT)
checks state and runs the tools; panel buttons are pressed through AT-SPI, the way a
screen reader would. Everything runs on its own workspace; Oumu's data, log and socket
live in the run folder. The user's AI settings are copied in (never printed) unless --no-ai.

Usage: oumu_e2e.py [--no-ai]       (CRATUS_PROFILE=fast|release picks the build)
"""
from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import desk  # noqa: E402
import oumu_probe as o  # noqa: E402

HERE = Path(__file__).parent
ROOT = HERE.parents[1]
WORKSPACE = "9"
TEXT = "Their is a mistake in this sentense. She dont like apples, and me and him goes to the store yesterday. In order to make a decision, we need to think about it."
EDITOR_RECT = (560, 260, 760, 360)
STALL_LIMIT_MS = 400
AI_PAUSE_S = 7  # stay under a free key's per-minute limit


class Run:
    def __init__(self) -> None:
        self.dir = ROOT / "target/e2e" / time.strftime("oumu-linux-%Y%m%d-%H%M%S")
        self.data = self.dir / "data"
        self.data.mkdir(parents=True)
        self.log = self.data / "oumu.log"
        self.results: list[tuple[str, bool, str]] = []

    def check(self, name: str, ok, detail="") -> bool:
        detail = o.scrub(str(detail))
        self.results.append((name, bool(ok), detail))
        print(f"{'ok  ' if ok else 'FAIL'} {name} {detail}", flush=True)
        return bool(ok)

    def log_lines(self) -> list[str]:
        try:
            return self.log.read_text(errors="replace").splitlines()
        except OSError:
            return []

    def wait_log(self, mark: int, pattern: str, secs: float):
        rx = re.compile(pattern)
        return o.wait(secs, lambda: next((l for l in reversed(self.log_lines()[mark:]) if rx.search(l)), None))

    def report(self) -> int:
        passed = sum(1 for r in self.results if r[1])
        md = f"# Oumu E2E (Linux) - {passed}/{len(self.results)} passed\n\nRun `{self.dir}`\n\n| Case | Result | Detail |\n|---|---|---|\n"
        md += "".join(f"| {n} | {'PASS' if ok else 'FAIL'} | {d.replace('|', '/')} |\n" for n, ok, d in self.results)
        (self.dir / "report.md").write_text(md)
        (ROOT / "target/e2e/oumu-linux-latest.md").write_text(md)
        print(f"\n{passed}/{len(self.results)} passed")
        return 0 if passed == len(self.results) else 1


# ---------------------------------------------------------------- AT-SPI (screen-reader path)

def atspi_app(pid: int):
    import gi

    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi

    desktop = Atspi.get_desktop(0)
    for i in range(desktop.get_child_count()):
        a = desktop.get_child_at_index(i)
        try:
            if a and a.get_process_id() == pid:
                return a
        except Exception:
            continue
    return None


def walk(node, depth=0):
    if node is None or depth > 60:
        return
    yield node
    try:
        n = node.get_child_count()
    except Exception:
        return
    for i in range(n):
        try:
            yield from walk(node.get_child_at_index(i), depth + 1)
        except Exception:
            continue


def buttons(pid: int):
    import gi

    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi

    app = atspi_app(pid)
    return [n for n in walk(app) if n.get_role() == Atspi.Role.PUSH_BUTTON] if app else []


def press(pid: int, name: str, secs: float = 8) -> bool:
    def find():
        app = atspi_app(pid)
        for n in walk(app):
            try:
                if (n.get_name() or "").strip() == name:
                    return n
            except Exception:
                continue
        return None

    node = o.wait(secs, find)
    if not node:
        return False
    try:
        act = node.get_action_iface()
        if act and act.get_n_actions() > 0:
            return bool(act.do_action(0))
    except Exception as e:  # noqa: BLE001
        print(f"  (AT-SPI action failed: {e})")
    return False


# ---------------------------------------------------------------- the suite

def editor_rect_center(st: dict, alert_id: str) -> tuple[int, int] | None:
    g = st["geometry"]
    r = next((r for r in g["rects"] if r["id"] == alert_id), None)
    if not r:
        return None
    return int(g["origin_x"] + r["x"] + r["w"] / 2), int(g["origin_y"] + r["y"] + r["h"] - 4)


def suite(run: Run, with_ai: bool) -> None:
    has_ai = o.settings(run.data, with_ai)
    app = o.start(run.data)
    mirror = run.dir / "mirror.txt"
    ed = None
    try:
        run.check("starts, probe answers", o.wait(30, lambda: o.api("/health")["ok"]))
        run.check("accessibility bus is on", o.wait(10, lambda: o.api("/health")["trusted"]))
        ed = subprocess.Popen([sys.executable, str(HERE / "fixtures/gtk_textview.py"), TEXT, str(mirror)], stderr=subprocess.DEVNULL)
        w = desk.wait_window(title="oumu-e2e")
        desk.place(w["address"], *EDITOR_RECT)
        desk.dispatch(f'hl.dsp.focus({{ window = "address:{w["address"]}" }})')
        st = o.wait(25, lambda: (lambda s: s if len(s.get("alerts", [])) >= 3 and len(s["geometry"]["rects"]) >= 3 else None)(o.api("/state")))
        st = st or o.api("/state")
        run.check("reads the field and finds mistakes", len(st.get("alerts", [])) >= 3, ", ".join(a["original"] for a in st.get("alerts", [])))
        run.check("underlines every mistake", len(st["geometry"]["rects"]) >= 3, f"{len(st['geometry']['rects'])} underlines")
        sc = st.get("score") or {}
        run.check("scores the text", (sc.get("words") or 0) > 20, f"words={sc.get('words')} overall={sc.get('overall')}")
        # the underline layer sits exactly over the editor (it once landed under the bar's zone)
        layer = next((l for l in desk.hypr("layers").splitlines() if "Oumu underlines" in l), "")
        m = re.search(r"xywh: (-?\d+) (-?\d+) (\d+) (\d+)", layer)
        g = st["geometry"]
        run.check("underline layer covers the field", m and (int(m[1]), int(m[2])) == (int(g["origin_x"]), int(g["origin_y"])), layer.strip()[:90])

        # hover "sentense" with the real pointer: the card opens; click Accept on it
        a = next((a for a in st["alerts"] if a["original"] == "sentense"), None)
        at = editor_rect_center(st, a["id"]) if a else None
        if at:
            gx, gy = at
            desk.jump_to(gx - 40, gy + 60)
            desk.move_to(gx, gy, steps=12)
            acc = run.wait_log(0, r"card-accept (\d+):(\d+)", 8)
            run.check("hover opens the suggestion card", acc, f"{gx},{gy}")
            if acc:
                ax, ay = map(int, re.search(r"card-accept (\d+):(\d+)", acc).groups())
                desk.move_to(ax, ay, steps=10)
                time.sleep(0.3)
                desk.button(True)
                time.sleep(0.06)
                desk.button(False)
                fixed = o.wait(10, lambda: "sentence." in o.api("/state")["doc"]["text"])
                run.check("Accept on the card fixes the word in the app", fixed and "sentence." in mirror.read_text(), "editor buffer has 'sentence'" if fixed else o.api("/state")["doc"]["text"][:60])
        else:
            run.check("hover opens the suggestion card", False, "no 'sentense' underline")
        desk.jump_to(200, 900)

        # hover every underline five times: the card opens each time, no stalls
        st = o.api("/state")
        marks = [editor_rect_center(st, r["id"]) for r in st["geometry"]["rects"]]
        before = len(run.log_lines())
        for _ in range(5):
            for hx, hy in marks:
                desk.jump_to(hx - 30, hy + 50)
                desk.move_to(hx, hy, steps=6)
                time.sleep(0.7)
                desk.move_to(hx - 30, hy + 160, steps=6)
                time.sleep(0.6)
        desk.jump_to(200, 900)
        stalls = [int(x) for x in re.findall(r"ui thread was busy for (\d+) ms", "\n".join(run.log_lines()[before:]))]
        worst = max(stalls, default=0)
        run.check("hovering many times stays smooth", worst < STALL_LIMIT_MS, f"{len(marks) * 5} hovers, worst stall {worst} ms")

        # the probe's Accept and Dismiss (the panel's path)
        st = o.wait(10, lambda: (lambda s: s if any(a["original"] == "Their" for a in s["alerts"]) else None)(o.api("/state")))
        their = next((a for a in (st or {}).get("alerts", []) if a["original"] == "Their"), None)
        if their:
            o.api("/accept", {"id": their["id"], "suggestion": 0})
            ok = o.wait(10, lambda: o.api("/state")["doc"]["text"].startswith("There is"))
            run.check("Accept (panel path) fixes 'Their'", ok, "There is a mistake..." if ok else "")
        else:
            run.check("Accept (panel path) fixes 'Their'", False, "no 'Their' alert")
        dont = next((a for a in o.api("/state")["alerts"] if a["original"] == "dont"), None)
        if dont:
            o.api("/dismiss", {"id": dont["id"]})
            run.check("Dismiss hides the suggestion", o.wait(5, lambda: not any(a["original"] == "dont" for a in o.api("/state")["alerts"])))

        panel_checks(run)
        offline_tools(run)
        if has_ai:
            ai_tools(run, app.pid)
            highlight(run)
        else:
            run.check("AI tools", True, "skipped (no AI key set up)")
    finally:
        desk.release_all()
        if ed:
            ed.terminate()
        app.terminate()
        try:
            app.wait(10)
        except subprocess.TimeoutExpired:
            app.kill()


def screen() -> tuple[int, int]:
    m = desk.hypr_json("monitors")[0]
    return int(m["width"] / m["scale"]), int(m["height"] / m["scale"])


def panel_window():
    return next((c for c in desk.hypr_json("clients") if c["title"] == "Oumu" and c["class"] != "oumu-e2e" and c.get("mapped", True)), None)


def panel_checks(run: Run) -> None:
    sw, _ = screen()
    second = o.start(run.data)
    try:
        second.wait(10)
    except subprocess.TimeoutExpired:
        second.kill()
    p = o.wait(8, lambda: (lambda s: s["panel"] if s["panel"]["visible"] and not s["panel"]["tucked"] else None)(o.api("/state")))
    run.check("a second launch opens the running Oumu's panel", p, f"x={p and p['x']} y={p and p['y']}")
    time.sleep(1.2)
    px = o.api("/state")["panel"]["x"]
    run.check("the panel opens fully on screen", -1 <= px < sw, f"x={px}")
    # where Hyprland really shows it: floating, pinned, at the wanted place
    want = o.api("/state")["panel"]
    win = o.wait(3, panel_window)
    placed = win and win["floating"] and win["pinned"] and abs(win["at"][0] - want["x"]) <= 4 and abs(win["at"][1] - want["y"]) <= 4
    run.check("Hyprland shows the panel where Oumu put it", placed, f"window at {win and win['at']} floating={win and win['floating']} pinned={win and win['pinned']}, wanted ({want['x']}, {want['y']})")
    for tucked, pause in ((True, 0.05), (False, 0.12), (True, 0.6), (False, 0.9)):
        o.api("/panel", {"tucked": tucked})
        time.sleep(pause)
    px = o.api("/state")["panel"]["x"]
    run.check("a tuck mid-slide doesn't leave it half open", -1 <= px < sw, f"x={px}")
    o.api("/panel", {"tucked": True})
    run.check("tucking hides the panel behind the edge tab", o.wait(5, lambda: (lambda s: s["tucked"] and s["tab_visible"])(o.api("/state")["panel"])))
    time.sleep(0.5)
    run.check("a tucked panel leaves no window behind", o.wait(3, lambda: panel_window() is None), "")


def offline_tools(run: Run) -> None:
    rw = o.api("/feature", {"name": "rewriter_scan", "args": {}})
    run.check("AI Rewriter scan (offline)", (rw or {}).get("words", 0) > 0, f"words={rw.get('words')} phrases={len(rw.get('phrases', []))}")
    sc = o.api("/feature", {"name": "score", "args": {}})
    run.check("Performance score", (sc or {}).get("words", 0) > 0, f"words={sc.get('words')}")
    for name, label in (("spot_check", "Spot check"), ("plagiarism", "Plagiarism check (web search)")):
        try:
            v = o.api("/feature", {"name": name, "args": {}})
            run.check(label, v is not None, json.dumps(v)[:80])
        except Exception as e:  # noqa: BLE001
            run.check(label, False, getattr(e, "read", lambda: str(e).encode())().decode(errors="replace")[:160])


def ai_tools(run: Run, pid: int) -> None:
    for name, args in (("ping_llm", {}), ("paraphrase", {"style": "Professional"}), ("translate", {"target": "Vietnamese"}), ("tone", {}), ("explain", {"whole": True}), ("detector", {}), ("humanize", {}), ("reactions", {}), ("summarize", {})):
        time.sleep(AI_PAUSE_S)
        try:
            v = json.dumps(o.api("/feature", {"name": name, "args": args}))
            run.check(f"AI {name}", len(v) > 10, v[:90])
        except Exception as e:  # noqa: BLE001
            body = e.read().decode(errors="replace") if hasattr(e, "read") else str(e)
            run.check(f"AI {name}", False, body[:160])

    # every tool from the panel itself: open its view, press its button through AT-SPI
    for view, button, feature in (("paraphraser", "Professional", "paraphrase"), ("humanizer", "The Naturalist", "humanize"), ("translate", "Translate all", "translate"), ("explain", "Explain the whole page", "explain"), ("detector", "Check this text", "detector"), ("reactions", "Predict reactions", "reactions"), ("tone", "Reload", "tone"), ("chat", "Summarize this page", "chat")):
        time.sleep(5)
        mark = len(run.log_lines())
        o.api("/panel", {"view": view, "focus": False})
        time.sleep(1.0)
        unnamed = sum(1 for b in buttons(pid) if not (b.get_name() or "").strip())
        run.check(f"buttons named: {view}", unnamed == 0, f"{unnamed} without a name")
        pressed = press(pid, button)
        done = run.wait_log(mark, rf"panel: {feature} done", 150) if pressed else None
        line = re.sub(r"^.*panel: ", "", done) if done else ("no answer in 150 s" if pressed else f"button '{button}' not found")
        run.check(f"panel {view}", done and "done ok" in done, line[:140])
    o.api("/panel", {"tucked": True})


def highlight(run: Run) -> None:
    """A highlight outside any text box (a web page): `oumu --translate` must translate
    exactly the highlighted sentence (Linux: the PRIMARY selection)."""
    sentence = "The weather is lovely today, so we are going for a long walk by the river."
    page = run.dir / "highlight.html"
    # Wayland: only a real pointer selection sets PRIMARY, so the test highlights the way a
    # user does (a triple-click on the sentence), not with a script
    page.write_text(f"<!doctype html><meta charset=utf-8><title>Oumu highlight test</title><body style='margin:0;font:22px sans-serif'><p style='position:absolute;left:40px;top:20px;margin:0'>Some text before.</p><p id=t style='position:absolute;left:40px;top:120px;margin:0;width:1200px'>{sentence}</p><p style='position:absolute;left:40px;top:220px;margin:0'>Some text after.</p>")
    browser = shutil.which("chromium")
    if not browser:
        run.check("highlight on a web page", False, "no Chromium found to test with")
        return
    prof = run.dir / "browser-profile"  # a throwaway profile: the user's own is never touched
    br = subprocess.Popen([browser, f"--user-data-dir={prof}", "--no-first-run", "--no-default-browser-check", "--ozone-platform=wayland", "--force-renderer-accessibility", f"--app=file://{page}"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        w = o.wait(25, lambda: next((c for c in desk.hypr_json("clients") if c["pid"] == br.pid or "Oumu highlight test" in c["title"]), None))
        if w:
            desk.place(w["address"], 300, 300, 1300, 320)
            desk.dispatch(f'hl.dsp.focus({{ window = "address:{w["address"]}" }})')
        time.sleep(3)
        w = next((c for c in desk.hypr_json("clients") if c["address"] == (w or {}).get("address")), w)
        if w:
            x, y = w["at"][0] + 150, w["at"][1] + 120 + 14
            desk.jump_to(x, y)
            for _ in range(3):  # triple-click: the whole paragraph
                desk.button(True)
                desk.button(False)
            time.sleep(0.5)
            desk.jump_to(200, 900)
        mark = len(run.log_lines())
        t = o.start(run.data, "--translate")
        try:
            t.wait(10)
        except subprocess.TimeoutExpired:
            t.kill()
        got = run.wait_log(mark, r"translate: [0-9]+ characters highlighted", 15)
        n = int(re.search(r"translate: ([0-9]+) characters", got).group(1)) if got else -1
        run.check("highlight on a web page is read", n == len(sentence), f"{n} characters (the sentence has {len(sentence)})")
        done = run.wait_log(mark, r"panel: translate done", 150)
        run.check("highlight on a web page is translated", done and "done ok" in done, re.sub(r"^.*panel: ", "", done) if done else "no answer")
    finally:
        br.terminate()
        subprocess.run(["pkill", "-f", str(prof)], check=False)


def main() -> int:
    with_ai = "--no-ai" not in sys.argv
    run = Run()
    desk.ensure_ydotoold()
    home = desk.current_workspace()
    desk.workspace(WORKSPACE)
    try:
        suite(run, with_ai)
    finally:
        desk.release_all()
        desk.workspace(home)
    return run.report()


if __name__ == "__main__":
    sys.exit(main())
