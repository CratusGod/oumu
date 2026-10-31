"""A huge paste into the Qt QTextEdit fixture: Oumu must stay responsive, mark the text as
truncated, keep its CPU use sane, and still underline what is on screen; then switching
fields and closing the window must clear its overlays."""
import subprocess
import sys
import time

import pyautogui
from pywinauto import Desktop

sys.path.insert(0, __file__.rsplit("\\", 1)[0])
from apps_windows import focused_exe, set_clipboard  # noqa: E402
from winprobe import oumu_windows, probe, wait_for  # noqa: E402

results = []


def note(name, ok, detail=""):
    results.append(ok)
    print(f"{'PASS' if ok else 'FAIL'} {name}: {detail}", flush=True)


def oumu_cpu_seconds() -> float:
    out = subprocess.run(["powershell", "-NoProfile", "-Command",
                          "(Get-Process oumu | Measure-Object -Property CPU -Sum).Sum"], capture_output=True, text=True).stdout
    return float(out.strip() or 0)


w = Desktop(backend="win32").window(title="oumu-e2e-rich")
w.set_focus()
r = w.rectangle()
pyautogui.click((r.left + r.right) // 2, (r.top + r.bottom) // 2)
time.sleep(0.3)
assert focused_exe() == "pythonw.exe", "focus is not in the fixture"
big = " ".join(f"Paragraph {i}: this is a example of a sentense." for i in range(1, 2001))  # ~94k chars
set_clipboard(big)
pyautogui.hotkey("ctrl", "a")
pyautogui.hotkey("ctrl", "v")
pyautogui.hotkey("ctrl", "home")
t0 = time.time()
s = wait_for("the big text", lambda: (lambda s: s if s.get("doc") and len(s["doc"]["text"]) > 50_000 else None)(probe("/state")),
             timeout=30)
note("a 94k-character text is read", True, f"in {time.time() - t0:.1f}s, {len(s['doc']['text'])} chars, "
     f"truncated={s['doc']['truncated']}")
note("it is marked as truncated (over 80,000 characters)", s["doc"]["truncated"] is True)
s = wait_for("underlines", lambda: (lambda s: s if s["geometry"]["rects"] else None)(probe("/state")), timeout=30)
note("on-screen mistakes are underlined", bool(s["geometry"]["rects"]), f"{len(s['alerts'])} alerts, "
     f"{len(s['geometry']['rects'])} underlines")

c0 = oumu_cpu_seconds()
t = time.time()
slow = 0
while time.time() - t < 10:
    q = time.time()
    probe("/health")
    slow = max(slow, time.time() - q)
    time.sleep(0.5)
cpu = (oumu_cpu_seconds() - c0) / (time.time() - t)
note("idle with the big text: CPU under 30% of a core", cpu < 0.30, f"{cpu * 100:.0f}% of a core")
note("the probe answers within 1 s throughout", slow < 1.0, f"slowest {slow:.2f}s")

# typing at the start of the big text (the part Oumu reads) still updates quickly
pyautogui.hotkey("ctrl", "home")
pyautogui.typewrite("Another sentense here. ", interval=0.02)
t0 = time.time()
try:
    wait_for("the new text", lambda: "Another sentense here" in probe("/state")["doc"]["text"] or None, timeout=15)
    note("typing in the big text updates it", True, f"{time.time() - t0:.1f}s")
except AssertionError as e:
    note("typing in the big text updates it", False, str(e)[:100])

# clear and close: the overlays on that field must go (the app now in front may get its own)
field = probe("/state")["doc"]["visible"]
pyautogui.hotkey("ctrl", "a")
pyautogui.press("delete")
time.sleep(1)
w.close()
time.sleep(2.5)
left = [x for x in oumu_windows() if (x["w"] == 60 and field["x"] <= x["x"] <= field["x"] + field["w"]
                                       and field["y"] <= x["y"] <= field["y"] + field["h"])
        or (x["click_through"] and (x["x"], x["y"], x["w"], x["h"]) == (field["x"], field["y"], field["w"], field["h"]))]
note("closing the window removes its button and underlines", not left, str(left[:2]))
print("DONE", sum(results), "/", len(results))
