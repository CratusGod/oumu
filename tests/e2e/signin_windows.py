"""Sign-in boxes (email, username, phone, "Email or phone") are never read; a normal box
on the same page still is. Uses the test page fixtures/signin.html in a Chromium browser.
    (run with Oumu's debug probe up, e.g. after qt_session.py start)"""
import subprocess
import sys
import time
from pathlib import Path

import pyautogui
from pywinauto import Desktop

HERE = Path(__file__).parent
sys.path.insert(0, str(HERE))
from apps_windows import browser_exe, click_center, focused_exe  # noqa: E402
from winprobe import probe  # noqa: E402

results = []


def note(name, ok, detail=""):
    results.append(ok)
    print(f"{'PASS' if ok else 'FAIL'} {name}: {detail}", flush=True)


exe = browser_exe()
subprocess.Popen([str(exe), "--new-window", (HERE / "fixtures" / "signin.html").resolve().as_uri()])
time.sleep(4)
handle = [w.handle for w in Desktop(backend="uia").windows() if w.window_text().startswith("Oumu sign-in check")][0]
win = Desktop(backend="uia").window(handle=handle)
try:
    for label, typed in (("Email", "nghia.test@example.com"), ("Username", "nghia_test"),
                         ("Số điện thoại", "0901234567"), ("Email or phone", "nghia.test@example.com")):
        win.set_focus()
        click_center(win.child_window(title=label, control_type="Edit").wrapper_object())
        assert focused_exe() == exe.name.lower()
        pyautogui.typewrite(typed, interval=0.02)
        time.sleep(2.5)
        d = probe("/state").get("doc") or {}
        read = d.get("active") and typed in d.get("text", "")
        note(f"'{label}' is not read", not read, f"doc: {d.get('role')} {d.get('text', '')[:30]!r}" if read else "ignored")
        pyautogui.hotkey("ctrl", "a")
        pyautogui.press("delete")
    win.set_focus()
    click_center(win.child_window(title="About you", control_type="Edit").wrapper_object())
    pyautogui.typewrite("I has a example of a sentense.", interval=0.02)
    time.sleep(3)
    s = probe("/state")
    d = s.get("doc") or {}
    note("'About you' is still checked", "sentense" in d.get("text", "") and bool(s["alerts"]),
         f"{len(s['alerts'])} alerts")
    pyautogui.hotkey("ctrl", "a")
    pyautogui.press("delete")
    # the address bar: URLs, not prose (Escape puts the page's address back; nothing is opened)
    win.set_focus()
    pyautogui.hotkey("ctrl", "l")
    time.sleep(0.4)
    pyautogui.typewrite("this is a example sentense", interval=0.02)
    time.sleep(2.5)
    d = probe("/state").get("doc") or {}
    read = d.get("active") and "example sentense" in d.get("text", "")
    note("the address bar is not read", not read, f"doc: {d.get('role')} {d.get('text', '')[:30]!r}" if read else "ignored")
    pyautogui.press("escape")
    pyautogui.press("escape")
finally:
    win.set_focus()
    pyautogui.hotkey("ctrl", "w")
print("DONE", sum(results), "/", len(results))
