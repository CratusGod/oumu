"""Oumu's Windows backend in real apps, beyond Notepad (see host_windows.py for that):
a Qt text edit, and a Chromium browser's textarea, single-line input and
contenteditable (the kind YouTube and Discord use). With --discord also Discord's
message box, in whatever channel is open: the text is typed, checked and cleared,
and Enter is never pressed, so nothing is ever sent.

Each field gets a sentence with an emoji before the mistake (apps count an emoji
as one or two UIA "characters"), and must: be read, have the mistake underlined
inside the field, take the fix by paste (the app's own undo step), and show it.

    python tests/e2e/apps_windows.py path\\to\\oumu.exe [out_dir] [--discord]

Needs pywinauto, pyautogui, Pillow and PySide6. Your clipboard is put back afterwards.
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
from pywinauto import Desktop

sys.path.insert(0, str(Path(__file__).parent))
from winprobe import PORT, probe, wait_for  # noqa: E402

HERE = Path(__file__).parent
EMOJI = "\U0001F600"
BEFORE, AFTER = "Great ", " work, but this sentense has a typo."
RESULTS: list[dict] = []
NO_WINDOW = subprocess.CREATE_NO_WINDOW


def ps(cmd: str) -> str:
    return subprocess.run(["powershell", "-NoProfile", "-Command", cmd], capture_output=True, text=True,
                          encoding="utf-8", creationflags=NO_WINDOW).stdout


def set_clipboard(text: str):
    ps(f"Set-Clipboard -Value '{text.replace(chr(39), chr(39) * 2)}'")


def type_sentence():
    """Type the test sentence, the emoji pasted (typing can't produce it)."""
    pyautogui.hotkey("ctrl", "a")
    pyautogui.press("delete")
    pyautogui.typewrite(BEFORE, interval=0.02)
    set_clipboard(EMOJI)
    pyautogui.hotkey("ctrl", "v")
    time.sleep(0.3)
    pyautogui.typewrite(AFTER, interval=0.02)


def clear_field():
    pyautogui.hotkey("ctrl", "a")
    pyautogui.press("delete")


def click_center(el):
    r = el.rectangle()
    pyautogui.click((r.left + r.right) // 2, (r.top + r.bottom) // 2)
    time.sleep(0.4)


def check_field(name: str, app: str, focus, out: Path, cleanup=clear_field):
    """focus() puts the caret in the field. Then: type, read, underline, fix, clean up."""
    t0 = time.time()
    status, detail = "PASS", ""
    try:
        focus()
        type_sentence()

        def seen():
            s = probe("/state")
            d = s.get("doc") or {}
            ok = d.get("bundle_id") == app and "sentense" in d.get("text", "") and (s.get("geometry") or {}).get("rects")
            return s if ok and any(a["original"] == "sentense" for a in s.get("alerts") or []) else None
        s = wait_for(f"{name}: Oumu to read and underline it", seen)
        vis, g = s["doc"]["visible"], s["geometry"]
        a = next(x for x in s["alerts"] if x["original"] == "sentense")
        rects = [r for r in g["rects"] if r["id"] == a["id"]]
        assert rects, "no underline for 'sentense'"
        for r in rects:
            assert 0 <= r["x"] and r["x"] + r["w"] <= vis["w"] + 1 and 0 <= r["y"] and r["y"] + r["h"] <= vis["h"] + 1, \
                f"underline {r} outside the field {vis}"
        ImageGrab.grab().save(out / "shots" / f"{name}_underlined.png")
        res = probe("/accept", {"id": a["id"], "suggestion": 0})
        assert res.get("method") == "paste", f"fixed by {res.get('method')} (whole-text replace: no undo step)"
        after = wait_for("the fix to show", lambda: (lambda d: d if d and "sentence" in d.get("text", "")
                                                     and "sentense" not in d["text"] else None)(probe("/state").get("doc")),
                         timeout=8)
        assert EMOJI in after["text"], "the emoji got lost"
        ImageGrab.grab().save(out / "shots" / f"{name}_fixed.png")
        detail = f"read as {s['doc']['role']}, {len(g['rects'])} underline(s), fixed by paste: {after['text'][:50]!r}"
    except Exception as e:
        status, detail = "FAIL", f"{type(e).__name__}: {e}"
        ImageGrab.grab().save(out / "shots" / f"{name}_FAIL.png")
    finally:
        try:
            cleanup()
        except Exception as e:
            print("cleanup:", e)
    RESULTS.append({"name": name, "status": status, "detail": detail, "seconds": round(time.time() - t0, 1)})
    print(f"{status:4} {name:28} {detail}", flush=True)


# ------------------------------------------------------------------ targets
def qt_target(out: Path):
    mirror = Path(tempfile.gettempdir()) / "oumu_qt_mirror.txt"
    pyw = Path(sys.executable).with_name("pythonw.exe")
    p = subprocess.Popen([str(pyw if pyw.exists() else sys.executable), "-I", str(HERE / "fixtures" / "qt_textedit.py"),
                          "", str(mirror)])  # pythonw has no console; CREATE_NO_WINDOW would hide the window
    exe = (pyw if pyw.exists() else Path(sys.executable)).name.lower()

    def focus():  # win32 lookup: a new Qt window shows up in UIA's top-level list late
        w = Desktop(backend="win32").window(title="oumu-e2e")
        w.set_focus()
        click_center(w)
    try:
        wait_for("the Qt window", lambda: Desktop(backend="win32").window(title="oumu-e2e").exists(timeout=0.2))
        check_field("qt_text_edit", exe, focus, out)
    finally:  # the whole tree: a venv's pythonw is a launcher that starts the real one
        subprocess.run(["taskkill", "/F", "/T", "/PID", str(p.pid)], capture_output=True)


def browser_exe() -> Path | None:
    local, pf, pf86 = (os.environ.get(k, "") for k in ("LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"))
    for p in (Path(local) / "imput/Helium/Application/chrome.exe", Path(pf) / "Google/Chrome/Application/chrome.exe",
              Path(pf86) / "Microsoft/Edge/Application/msedge.exe", Path(pf) / "Microsoft/Edge/Application/msedge.exe"):
        if p.is_file():
            return p
    return None


def browser_targets(out: Path):
    exe = browser_exe()
    if exe is None:
        RESULTS.append({"name": "browser", "status": "SKIP", "detail": "no Chromium browser found", "seconds": 0})
        return
    page = (HERE / "fixtures" / "fields.html").resolve().as_uri()
    subprocess.Popen([str(exe), "--new-window", page])
    handle = wait_for("the test page", lambda: [w.handle for w in Desktop(backend="uia").windows()
                                               if w.window_text().startswith("Oumu field check")], timeout=20)[0]
    win = Desktop(backend="uia").window(handle=handle)
    try:
        for name, label in (("browser_textarea", "Textarea"), ("browser_input", "Single-line input"),
                            ("browser_contenteditable", "Contenteditable (like YouTube and Discord)")):
            def focus(label=label):
                win.set_focus()
                # textarea, input and role=textbox are all Edit controls; the <label> is Text
                click_center(win.child_window(title=label, control_type="Edit").wrapper_object())
            check_field(name, exe.name.lower(), focus, out)
    finally:
        win.set_focus()
        pyautogui.hotkey("ctrl", "w")  # only the window this test opened


def discord_target(out: Path):
    upd = Path(os.environ.get("LOCALAPPDATA", "")) / "Discord" / "Update.exe"
    if not upd.is_file():
        RESULTS.append({"name": "discord", "status": "SKIP", "detail": "Discord is not installed", "seconds": 0})
        return
    subprocess.Popen([str(upd), "--processStart", "Discord.exe"])

    def message_box():
        """Discord's box is named "Message #channel" / "Message @friend"."""
        for w in Desktop(backend="uia").windows(class_name="Chrome_WidgetWin_1"):
            try:
                for e in w.descendants(control_type="Edit") + w.descendants(control_type="Group"):
                    if (e.element_info.name or "").startswith("Message "):
                        return w, e
            except Exception:
                continue
        return None
    w, box = wait_for("Discord's message box", message_box, timeout=20)

    def focus():
        w.set_focus()
        click_center(box)
    check_field("discord_message_box", "discord.exe", focus, out)


def main():
    exe = Path(sys.argv[1]).resolve()
    out = Path(next((a for a in sys.argv[2:] if not a.startswith("--")), "e2e_out_windows_apps")).resolve()
    (out / "shots").mkdir(parents=True, exist_ok=True)
    clip = ps("Get-Clipboard -Raw")
    env = dict(os.environ, OUMU_DEBUG_PORT=str(PORT))
    oumu = subprocess.Popen([str(exe)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                            creationflags=NO_WINDOW)
    try:
        wait_for("Oumu's debug probe", lambda: probe("/health").get("trusted"))
        qt_target(out)
        browser_targets(out)
        if "--discord" in sys.argv:
            discord_target(out)
    except Exception:
        import traceback
        traceback.print_exc()
        RESULTS.append({"name": "run", "status": "FAIL", "detail": "aborted, see the traceback", "seconds": 0})
    finally:
        subprocess.run(["taskkill", "/F", "/T", "/PID", str(oumu.pid)], capture_output=True)
        if clip:
            set_clipboard(clip.rstrip("\r\n"))
        passed = sum(r["status"] == "PASS" for r in RESULTS)
        ran = sum(r["status"] != "SKIP" for r in RESULTS)
        (out / "results.json").write_text(json.dumps(RESULTS, indent=1), encoding="utf-8")
        lines = [f"# Oumu in Windows apps: {passed}/{ran} passed", "", "| Field | Result | Detail |", "|---|---|---|"]
        lines += [f"| {r['name']} | {dict(PASS='✅', FAIL='❌').get(r['status'], '⏭')} | {r['detail']} |" for r in RESULTS]
        (out / "results.md").write_text("\n".join(lines), encoding="utf-8")
        print(f"DONE {passed}/{ran} passed")
        sys.exit(0 if passed == ran else 1)


if __name__ == "__main__":
    main()
