"""Type TEXT into the Qt fixture (only after checking the focus is really there).
    python qt_type.py "TEXT\""""
import sys
import time

import pyautogui
from pywinauto import Desktop

sys.path.insert(0, __file__.rsplit("\\", 1)[0])
from apps_windows import focused_exe  # noqa: E402
from winprobe import summary  # noqa: E402

w = Desktop(backend="win32").window(title="oumu-e2e")
w.set_focus()
r = w.rectangle()
pyautogui.click((r.left + r.right) // 2, (r.top + r.bottom) // 2)
time.sleep(0.3)
assert focused_exe() == "pythonw.exe", f"focus is in {focused_exe()}"
pyautogui.hotkey("ctrl", "a")
pyautogui.press("delete")
pyautogui.typewrite(sys.argv[1], interval=0.01)
time.sleep(3)
print(summary())
