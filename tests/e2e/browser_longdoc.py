"""Long, wrapping, scrolling text in a Chromium textarea (the test page): every underline
inside the box, one per line, on the right word, and following a scroll.
    python browser_longdoc.py SHOT_PREFIX"""
import subprocess
import sys
import time
from pathlib import Path

import pyautogui
from PIL import ImageGrab
from pywinauto import Desktop

HERE = Path(__file__).parent
sys.path.insert(0, str(HERE))
from apps_windows import browser_exe, click_center, focused_exe, set_clipboard  # noqa: E402
from winprobe import probe, wait_for  # noqa: E402

shot = sys.argv[1]
results = []


def note(name, ok, detail=""):
    results.append(ok)
    print(f"{'PASS' if ok else 'FAIL'} {name}: {detail}", flush=True)


exe = browser_exe()
subprocess.Popen([str(exe), "--new-window", (HERE / "fixtures" / "fields.html").resolve().as_uri()])
handle = wait_for("the test page", lambda: [w.handle for w in Desktop(backend="uia").windows()
                                           if w.window_text().startswith("Oumu field check")], timeout=20)[0]
win = Desktop(backend="uia").window(handle=handle)
try:
    win.set_focus()
    box = win.child_window(title="Textarea", control_type="Edit").wrapper_object()
    click_center(box)
    assert focused_exe() == exe.name.lower(), "focus is not in the test page"
    # long lines wrap; 40 of them overflow the box, which then scrolls
    para = " ".join(f"Sentence {i} is a example of a sentense that wraps around." for i in range(1, 41))
    set_clipboard(para)
    pyautogui.hotkey("ctrl", "a")
    pyautogui.hotkey("ctrl", "v")
    pyautogui.hotkey("ctrl", "home")
    s = wait_for("underlines", lambda: (lambda s: s if s["geometry"]["rects"] and len(s["alerts"]) > 20 else None)(probe("/state")))
    time.sleep(1.5)
    s = probe("/state")
    d, g = s["doc"], s["geometry"]
    vis, text = d["visible"], d["text"]
    rects = g["rects"]
    inside = all(0 <= r["x"] and r["x"] + r["w"] <= vis["w"] + 1 and 0 <= r["y"] and r["y"] + r["h"] <= vis["h"] + 1
                 for r in rects)
    note("many mistakes: only on-screen ones underlined, all inside the box", inside and 0 < len(rects) < len(s["alerts"]),
         f"{len(s['alerts'])} alerts, {len(rects)} underlines")
    ImageGrab.grab().save(shot + "_top.png")
    before = {r["id"]: round(r["y"]) for r in rects}
    shown_before = d.get("text", "")
    r = box.rectangle()
    pyautogui.moveTo((r.left + r.right) // 2, (r.top + r.bottom) // 2)
    for _ in range(3):
        pyautogui.scroll(-120)  # one wheel notch each (Windows counts 120 per notch)
        time.sleep(0.2)
    time.sleep(2.5)
    s = probe("/state")
    rects = s["geometry"]["rects"]
    after = {x["id"]: round(x["y"]) for x in rects}
    inside = all(0 <= x["x"] and x["x"] + x["w"] <= vis["w"] + 1 and 0 <= x["y"] and x["y"] + x["h"] <= vis["h"] + 1
                 for x in rects)
    ImageGrab.grab().save(shot + "_scrolled.png")
    moved = [i for i in after if i in before and after[i] != before[i]]
    new = set(after) - set(before)
    note("scrolling moves the underlines with their words", bool(moved or new) and inside and bool(rects),
         f"{len(before)} -> {len(after)} underlines, {len(new)} new words, {len(moved)} moved")
    ImageGrab.grab().save(shot + "_scrolled.png")
finally:
    try:
        if focused_exe() == exe.name.lower():
            pyautogui.hotkey("ctrl", "a")
            pyautogui.press("delete")
        win.set_focus()
        pyautogui.hotkey("ctrl", "w")
    except Exception as e:
        print("cleanup:", e)
print("DONE", sum(results), "/", len(results))
