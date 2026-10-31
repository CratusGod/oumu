"""Long text in a Qt fixture window: underlines must stay on their words while scrolling
(with the pointer on empty space and on an underline) and moving the window, and hide
when another app is in front.
    python qt_longdoc.py WINDOW_TITLE SHOT_PREFIX"""
import subprocess
import sys
import time

import pyautogui
from PIL import ImageGrab
from pywinauto import Desktop

sys.path.insert(0, __file__.rsplit("\\", 1)[0])
from apps_windows import focused_exe, set_clipboard  # noqa: E402
from winprobe import oumu_windows, probe  # noqa: E402

title, shot = sys.argv[1], sys.argv[2]
LINES = [f"Line {i}: this is a example of a sentense number {i}." for i in range(1, 61)]
results = []


def note(name, ok, detail=""):
    results.append((name, ok))
    print(f"{'PASS' if ok else 'FAIL'} {name}: {detail}", flush=True)


def state():
    s = probe("/state")
    vis = s["doc"]["visible"]
    rects = s["geometry"]["rects"]
    inside = all(0 <= r["x"] and r["x"] + r["w"] <= vis["w"] + 1 and 0 <= r["y"] and r["y"] + r["h"] <= vis["h"] + 1
                 for r in rects)
    return s, rects, inside


def first_y(rects):
    """Where each underlined word is (id -> y): changes when the text scrolls."""
    return {r["id"]: round(r["y"]) for r in rects}


w = Desktop(backend="win32").window(title=title)
w.set_focus()
r = w.rectangle()
pyautogui.click((r.left + r.right) // 2, (r.top + r.bottom) // 2)
time.sleep(0.3)
assert focused_exe() == "pythonw.exe", "focus is not in the fixture"
pyautogui.hotkey("ctrl", "a")
set_clipboard("\r\n".join(LINES))
pyautogui.hotkey("ctrl", "v")
pyautogui.hotkey("ctrl", "home")
time.sleep(4)
s, rects, inside = state()
ys = sorted({round(x["y"]) for x in rects})
note("long text: underlines on separate lines, inside the field", inside and len(ys) == len(rects) or not rects,
     f"{len(s['alerts'])} alerts, {len(rects)} underlines on {len(ys)} line heights")
ImageGrab.grab(bbox=(r.left, r.top, r.right, r.bottom)).save(shot + "_top.png")

# scroll with the pointer on empty space (right of the text)
y0 = first_y(rects)
pyautogui.moveTo(r.right - 80, (r.top + r.bottom) // 2, duration=0.2)
pyautogui.scroll(-5)
time.sleep(2.5)
s, rects, inside = state()
moved = first_y(rects) != y0 or not rects
note("scroll on empty space moves the text and its underlines", moved and inside, f"{len(y0)} -> {len(first_y(rects))} underlines")
ImageGrab.grab(bbox=(r.left, r.top, r.right, r.bottom)).save(shot + "_scrolled.png")

# scroll with the pointer resting on an underline
if rects:
    u = rects[len(rects) // 2]
    ox, oy = s["geometry"]["origin_x"], s["geometry"]["origin_y"]
    pyautogui.moveTo(ox + u["x"] + u["w"] / 2, oy + u["y"] + u["h"] / 2, duration=0.3)
    time.sleep(1.5)
    y1 = first_y(rects)
    pyautogui.scroll(-5)
    time.sleep(2.5)
    s, rects, inside = state()
    note("scroll with the pointer on an underline still scrolls", first_y(rects) != y1 and inside,
         f"{len(y1)} -> {len(first_y(rects))} underlines")
    pyautogui.moveTo(r.right - 80, r.bottom - 40, duration=0.2)
    time.sleep(1)

# move the window: the overlay follows
w.move_window(r.left - 150, r.top + 40)
time.sleep(2.5)
v = probe("/state")["doc"]["visible"]
ul = [x for x in oumu_windows() if x["click_through"] and x["w"] > 300]
note("the overlay follows a moved window", bool(ul) and abs(ul[0]["x"] - v["x"]) <= 2 and abs(ul[0]["y"] - v["y"]) <= 2,
     f"field {v}, overlay {ul[:1]}")
w.move_window(r.left, r.top)
time.sleep(1)

# another app in front: a File Explorer window (nothing is typed there)
subprocess.Popen(["explorer.exe", "shell:Personal"])
time.sleep(3)
front = probe("/state")["status"]["frontmost_bid"]
ul = [x for x in oumu_windows() if x["click_through"] and x["w"] > 300]
note("underlines hide when another app is in front", front == "explorer.exe" and not ul, f"front {front}, overlay {ul[:1]}")
for ex in Desktop(backend="uia").windows(class_name="CabinetWClass"):
    if "Documents" in ex.window_text():
        ex.close()
print("DONE", sum(ok for _, ok in results), "/", len(results))


