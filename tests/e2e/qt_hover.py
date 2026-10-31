"""The suggestion card with the real mouse, in the Qt fixture (QTextEdit):
hover an underline -> the card opens; a click on the word itself reaches the app;
Accept in the card fixes the word; then the overlay lets clicks through again.
    python qt_hover.py SHOT_PREFIX"""
import sys
import time

import pyautogui
from PIL import ImageGrab
from pywinauto import Desktop

sys.path.insert(0, __file__.rsplit("\\", 1)[0])
from apps_windows import focused_exe  # noqa: E402
from winprobe import oumu_windows, probe, wait_for  # noqa: E402

shot = sys.argv[1]
results = []


def note(name, ok, detail=""):
    results.append(ok)
    print(f"{'PASS' if ok else 'FAIL'} {name}: {detail}", flush=True)


w = Desktop(backend="win32").window(title="oumu-e2e-rich")
w.set_focus()
r = w.rectangle()
pyautogui.click((r.left + r.right) // 2, (r.top + r.bottom) // 2)
time.sleep(0.3)
assert focused_exe() == "pythonw.exe", "focus is not in the fixture"
pyautogui.hotkey("ctrl", "a")
pyautogui.press("delete")
pyautogui.typewrite("This is a example of a sentense that need some fixes.", interval=0.01)
s = wait_for("the underline", lambda: (lambda s: s if any(a["original"] == "sentense" for a in s["alerts"])
                                       and s["geometry"]["rects"] else None)(probe("/state")))
a = next(x for x in s["alerts"] if x["original"] == "sentense")
u = next(x for x in s["geometry"]["rects"] if x["id"] == a["id"])
ox, oy = s["geometry"]["origin_x"], s["geometry"]["origin_y"]
wx, wy = ox + u["x"] + u["w"] / 2, oy + u["y"] + u["h"] / 2

pyautogui.moveTo(wx, wy, duration=0.3)
cards = wait_for("the card", lambda: probe("/state")["card_rects"], timeout=5)
note("hovering an underline opens its card", bool(cards), f"card at {cards[0]}")
ul = [x for x in oumu_windows() if x["w"] > 300 and not (x["w"] >= 400 and x["h"] >= 700)]
note("over the word the overlay still lets the pointer through", all(x["click_through"] for x in ul), str(ul[:1]))

# a click on the word reaches the app: the caret moves into "sentense"
pyautogui.click(wx, wy)
time.sleep(1)
sel = probe("/state")["doc"]["sel_start"]
note("a click on the underlined word reaches the app", a["u16_start"] <= sel <= a["u16_end"],
     f"caret at {sel}, word {a['u16_start']}-{a['u16_end']}")

# into the card and Accept (the first button in the card's action row)
pyautogui.moveTo(wx, wy, duration=0.2)
cards = wait_for("the card again", lambda: probe("/state")["card_rects"], timeout=5)
c = cards[0]
ImageGrab.grab().save(shot + "_card.png")
pyautogui.moveTo(ox + c["x"] + 60, oy + c["y"] + c["h"] / 2, duration=0.3)
time.sleep(0.6)
ul = [x for x in oumu_windows() if x["w"] > 300 and not (x["w"] >= 400 and x["h"] >= 700)]
note("over the card the overlay takes the pointer", any(not x["click_through"] for x in ul), str(ul[:1]))
# Accept: the card's layout puts it 52 px right and 128 px down from the card's corner
# (WebView2 does not expose the overlay's buttons to UI Automation reliably)
pyautogui.moveTo(ox + c["x"] + 52, oy + c["y"] + 128, duration=0.2)
time.sleep(0.3)
pyautogui.click()
try:
    done = wait_for("the fix", lambda: (lambda d: d if "sentence" in d["text"] and "sentense" not in d["text"] else None)(
        probe("/state")["doc"]), timeout=6)
    note("Accept in the card fixes the word", True, done["text"][:60])
except AssertionError as e:
    ImageGrab.grab().save(shot + "_accept_FAIL.png")
    note("Accept in the card fixes the word", False, str(e)[:120])
pyautogui.moveTo(r.right - 60, r.bottom - 60, duration=0.3)
time.sleep(1.2)
ul = [x for x in oumu_windows() if x["w"] > 300 and not (x["w"] >= 400 and x["h"] >= 700)]
note("afterwards the overlay lets clicks through", all(x["click_through"] for x in ul), str(ul[:1]))
print("DONE", sum(results), "/", len(results))
