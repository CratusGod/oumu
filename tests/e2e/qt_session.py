"""Start (or reuse) a hidden Oumu with its debug probe plus the Qt fixture window, for
interactive checks that must never touch the user's own apps.
    python qt_session.py start|stop"""
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
from winprobe import PORT, probe, wait_for  # noqa: E402

if sys.argv[1] == "stop":
    subprocess.run(["taskkill", "/F", "/IM", "oumu.exe"], capture_output=True)
    out = subprocess.run(["powershell", "-NoProfile", "-Command",
                          "Get-CimInstance Win32_Process -Filter \"Name like 'python%'\" | Where-Object { $_.CommandLine "
                          "-like '*qt_textedit*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }"],
                         capture_output=True, text=True)
    sys.exit(0)

try:
    probe("/health", timeout=1)
except Exception:
    subprocess.Popen([str(ROOT / "src-tauri/target/debug/oumu.exe")], env=dict(os.environ, OUMU_DEBUG_PORT=str(PORT)),
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, creationflags=subprocess.CREATE_NO_WINDOW)
    wait_for("Oumu", lambda: probe("/health").get("trusted"))
pyw = Path(sys.executable).with_name("pythonw.exe")
rich = sys.argv[1] == "start-rich"
subprocess.Popen([str(pyw), "-I", str(HERE / "fixtures/qt_textedit.py"), "", str(Path(tempfile.gettempdir()) / "oumu_qt_mirror.txt")],
                 env=dict(os.environ, OUMU_QT_RICH="1" if rich else "0"))
from pywinauto import Desktop  # noqa: E402
title = "oumu-e2e-rich" if rich else "oumu-e2e"
wait_for("the Qt window", lambda: Desktop(backend="win32").window(title=title).exists(timeout=0.2))
time.sleep(1)
print("ready")
