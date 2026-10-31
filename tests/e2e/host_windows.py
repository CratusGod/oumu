"""End-to-end check of Oumu's Windows backend on a real desktop, through Notepad.

Opens a scratch text file with mistakes in Notepad, starts Oumu with its debug
probe, and checks through UI Automation that Oumu reads the text, finds the
mistakes, places underlines inside Notepad's text area, and applies a fix that
Notepad really shows. The file is saved and closed afterwards.

    python tests/e2e/host_windows.py path\\to\\oumu.exe [out_dir]

The exe must have its UI built in (`cargo build --features tauri/custom-protocol`,
or `tauri build`): a plain debug build loads the UI from the dev server.

Needs: pywinauto, pyautogui, Pillow. Stops early, touching nothing, when
Notepad is already open (it must not take over someone's Notepad).
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import pyautogui
from PIL import ImageGrab

sys.path.insert(0, str(Path(__file__).parent))
from winprobe import PORT, probe, wait_for  # noqa: E402


# The emoji before the mistake checks that offsets survive a character that is two
# UTF-16 units (apps count it as one or two UIA "characters").
TEXT = "This is a example 😀 of a sentense that need some fixes.\rOumu should underline the mistakes."
RESULTS: list[dict] = []


def step(name: str, fn):
    t0 = time.time()
    try:
        detail = fn() or ""
        status = "PASS"
    except Exception as e:
        detail, status = f"{type(e).__name__}: {e}", "FAIL"
    RESULTS.append({"name": name, "status": status, "detail": str(detail), "seconds": round(time.time() - t0, 1)})
    print(f"{status:4} {name:40} {detail}", flush=True)
    return status == "PASS"


def notepad_running() -> bool:
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq notepad.exe", "/NH"], capture_output=True, text=True).stdout
    return "notepad.exe" in out.lower()


def main():
    exe = Path(sys.argv[1]).resolve()
    out = Path(sys.argv[2] if len(sys.argv) > 2 else "e2e_out_windows").resolve()
    (out / "shots").mkdir(parents=True, exist_ok=True)
    if notepad_running():
        print("Notepad is open: close it and run again (this test drives Notepad).")
        sys.exit(2)

    from pywinauto import Application, Desktop

    scratch = Path(tempfile.gettempdir()) / "oumu-e2e" / "oumu check.txt"
    scratch.parent.mkdir(exist_ok=True)
    scratch.write_text(TEXT.replace("\r", "\r\n"), encoding="utf-8")
    env = dict(os.environ, OUMU_DEBUG_PORT=str(PORT), RUST_LOG="info")
    oumu = subprocess.Popen([str(exe)], env=env, stdout=open(out / "oumu_log.txt", "w"), stderr=subprocess.STDOUT,
                            creationflags=subprocess.CREATE_NO_WINDOW)  # a debug build has a console
    pad = None
    state: dict = {}
    try:
        step("Oumu starts, UI Automation available",
             lambda: f"trusted={wait_for('the debug probe', lambda: probe('/health'))['trusted']}")

        def open_notepad():
            nonlocal pad
            subprocess.Popen(["notepad.exe", str(scratch)])
            win = wait_for("Notepad", lambda: [w for w in Desktop(backend="uia").windows()
                                               if "oumu check" in w.window_text().lower()])[0]
            pad = Application(backend="uia").connect(handle=win.handle)
            w = pad.window(handle=win.handle)
            w.set_focus()
            r = w.rectangle()
            pyautogui.click((r.left + r.right) // 2, (r.top + r.bottom) // 2)
            pyautogui.hotkey("ctrl", "end")
            return w.window_text()
        step("Notepad opens the scratch file", open_notepad)

        def read_doc():
            nonlocal state
            def ready():
                s = probe("/state")
                d = s.get("doc") or {}
                return s if d.get("bundle_id") == "notepad.exe" and "sentense" in d.get("text", "") else None
            state = wait_for("Oumu to read Notepad", ready)
            d = state["doc"]
            return f"{d['app_name']} · role {d['role']} · {len(d['text'])} chars · frame {d['frame']}"
        step("Oumu reads Notepad's text", read_doc)

        def find_mistakes():
            nonlocal state
            def found():
                s = probe("/state")
                return s if len(s.get("alerts") or []) >= 2 else None
            state = wait_for("alerts", found)
            return ", ".join(f"{a['original']}→{(a['suggestions'] or ['?'])[0]}" for a in state["alerts"][:4])
        step("Oumu finds the mistakes", find_mistakes)

        def underlines():
            nonlocal state
            def placed():
                s = probe("/state")
                return s if (s.get("geometry") or {}).get("rects") else None
            state = wait_for("underline rects", placed)
            f = state["doc"]["visible"]
            rects = state["geometry"]["rects"]
            ox, oy = state["geometry"]["origin_x"], state["geometry"]["origin_y"]
            inside = [r for r in rects
                      if f["x"] - 2 <= ox + r["x"] and ox + r["x"] + r["w"] <= f["x"] + f["w"] + 2
                      and f["y"] - 2 <= oy + r["y"] and oy + r["y"] + r["h"] <= f["y"] + f["h"] + 2]
            time.sleep(0.6)
            ImageGrab.grab().save(out / "shots" / "notepad_underlines.png")
            assert len(inside) == len(rects), f"{len(rects) - len(inside)} of {len(rects)} underlines outside the text area"
            return f"{len(rects)} underline(s), all inside Notepad's text area"
        step("Underlines sit on the words", underlines)

        def notepad_text() -> str:
            return pad.window(title_re=".*oumu check.*").child_window(control_type="Document").window_text()

        def accept_fix():
            alerts = probe("/state")["alerts"]
            a = next((x for x in alerts if x["original"].lower() == "sentense"), None)
            assert a and a["suggestions"], f"no fix offered for 'sentense' in {[x['original'] for x in alerts]}"
            fix = a["suggestions"][0]
            r = probe("/accept", {"id": a["id"], "suggestion": 0})
            now = wait_for("Notepad to show the fix",
                           lambda: (lambda t: t if fix in t and "sentense" not in t else None)(notepad_text()), timeout=8)
            ImageGrab.grab().save(out / "shots" / "notepad_fixed.png")
            return f"sentense → {fix} via {r.get('method')}; Notepad now: {now.splitlines()[0][:60]!r}"
        step("A fix is applied in Notepad", accept_fix)

        def undo_works():
            w = pad.window(title_re=".*oumu check.*")
            w.set_focus()
            pyautogui.hotkey("ctrl", "z")
            time.sleep(0.6)
            txt = notepad_text()
            assert "sentense" in txt, f"after Ctrl+Z: {txt[:60]!r}"
            pyautogui.hotkey("ctrl", "y")
            return "Ctrl+Z brings the original word back (one undo step)"
        step("The fix is one undo step", undo_works)
    finally:
        try:
            if pad:
                w = pad.window(title_re=".*oumu check.*")
                w.set_focus()
                pyautogui.hotkey("ctrl", "s")
                time.sleep(0.6)
                pyautogui.hotkey("ctrl", "w")  # only our tab: Notepad may have restored other tabs of yours
                time.sleep(0.6)
        except Exception as e:
            print("closing Notepad:", e)
        subprocess.run(["taskkill", "/F", "/T", "/PID", str(oumu.pid)], capture_output=True)
        try:
            scratch.unlink()
        except OSError:
            pass
        passed = sum(r["status"] == "PASS" for r in RESULTS)
        (out / "results.json").write_text(json.dumps(RESULTS, indent=1), encoding="utf-8")
        lines = [f"# Oumu on Windows: {passed}/{len(RESULTS)} passed", "", "| Step | Result | Detail |", "|---|---|---|"]
        lines += [f"| {r['name']} | {'✅' if r['status'] == 'PASS' else '❌'} | {r['detail']} |" for r in RESULTS]
        (out / "results.md").write_text("\n".join(lines), encoding="utf-8")
        print(f"DONE {passed}/{len(RESULTS)} passed")
        sys.exit(0 if passed == len(RESULTS) else 1)


if __name__ == "__main__":
    main()
