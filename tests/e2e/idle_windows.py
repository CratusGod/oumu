"""End-to-end check that Oumu rests on Windows and wakes when it should.

1. Idle: with a window that has no text field in front, the host loop ticks
   about once a second (it ran 10 times a second before).
2. Wake: bringing a text field back to the front gets it read and its
   underlines placed again quickly (the focus hook wakes the loop).
3. Fullscreen: a fullscreen window in front hides every Oumu overlay (the
   edge tab included) and keeps the loop slow; they come back afterwards.

    python tests/e2e/idle_windows.py path\\to\\oumu.exe [out_dir]

Types nothing and opens no app of yours: the text field is a small WinForms
window built here with PowerShell (Notepad would restore your session tabs).
Needs: pywinauto.
"""
from __future__ import annotations

import json
import os
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from winprobe import PORT, oumu_windows, probe, wait_for  # noqa: E402

TEXT = "This is a example of a sentense that need some fixes.\r\nOumu should underline the mistakes."
RESULTS: list[dict] = []

# A window with no text field, plain or fullscreen, titled so it can be found.
TK = r"""
import sys, tkinter as tk
root = tk.Tk(); root.title(sys.argv[1]); root.geometry("420x300+200+200")
tk.Label(root, text="Oumu idle check: no text field here").pack(expand=True)
if sys.argv[2] == "full":
    root.attributes("-fullscreen", True)
root.mainloop()
"""


def step(name: str, fn):
    t0 = time.time()
    try:
        detail = fn() or ""
        status = "PASS"
    except Exception as e:
        detail, status = f"{type(e).__name__}: {e}", "FAIL"
    RESULTS.append({"name": name, "status": status, "detail": str(detail), "seconds": round(time.time() - t0, 1)})
    print(f"{status:4} {name:44} {detail}", flush=True)
    return status == "PASS"


FIELD_CS = r"""
using System; using System.Windows.Forms;
static class P {
  [STAThread] static void Main(string[] a) {
    var f = new Form { Text = "oumu idle check", Width = 900, Height = 420, Left = 300, Top = 200, StartPosition = FormStartPosition.Manual };
    var t = new TextBox { Multiline = true, Dock = DockStyle.Fill, Text = System.IO.File.ReadAllText(a[0]),
                          Font = new System.Drawing.Font("Segoe UI", 14) };
    f.Controls.Add(t);
    f.Shown += (s, e) => { t.Focus(); t.SelectionStart = t.Text.Length; };
    Application.Run(f);
  }
}
"""


def build_field(out: Path) -> Path:
    """A one-text-box window, compiled once with Windows PowerShell's Add-Type."""
    exe = out / "oumu_field.exe"
    if not exe.exists():
        src = out / "oumu_field.cs"
        src.write_text(FIELD_CS, encoding="utf-8")
        ps = (f"Add-Type -TypeDefinition (Get-Content -Raw '{src}') -ReferencedAssemblies System.Windows.Forms,System.Drawing "
              f"-OutputAssembly '{exe}' -OutputType WindowsApplication")
        subprocess.run(["powershell", "-NoProfile", "-Command", ps], check=True, capture_output=True)
    return exe


def rate(seconds: float = 5.0) -> tuple[float, int, int]:
    """Host ticks per second, UI Automation calls and loop wakes over `seconds`."""
    a = probe("/state")["ipc"]
    time.sleep(seconds)
    b = probe("/state")["ipc"]
    return (b["ticks"] - a["ticks"]) / seconds, b["a11y_calls"] - a["a11y_calls"], b["wakes"] - a["wakes"]


def field_read():
    """The state once the test field is in front, read and checked."""
    s = probe("/state")
    d = s.get("doc") or {}
    ok = d.get("active") and d.get("bundle_id") == "oumu_field.exe" and "sentense" in d.get("text", "") and s.get("alerts")
    return s if ok else None


def main():
    exe = Path(sys.argv[1]).resolve()
    out = Path(sys.argv[2] if len(sys.argv) > 2 else "e2e_out_idle").resolve()
    out.mkdir(parents=True, exist_ok=True)
    field_exe = build_field(out)

    from pywinauto import Application, Desktop

    def find(title: str):
        return wait_for(title, lambda: [w for w in Desktop(backend="uia").windows() if title in w.window_text()])[0]

    def front(win):
        w = Application(backend="uia").connect(handle=win.handle).window(handle=win.handle)
        w.set_focus()
        return w

    scratch = Path(tempfile.gettempdir()) / "oumu-e2e" / "oumu idle check.txt"
    scratch.parent.mkdir(exist_ok=True)
    scratch.write_text(TEXT, encoding="utf-8")
    env = dict(os.environ, OUMU_DEBUG_PORT=str(PORT), RUST_LOG="info")
    oumu = subprocess.Popen([str(exe)], env=env, stdout=open(out / "oumu_log.txt", "w"), stderr=subprocess.STDOUT,
                            creationflags=subprocess.CREATE_NO_WINDOW)
    tk_procs: list[subprocess.Popen] = []
    pad_win = None
    field = None
    try:
        step("Oumu starts", lambda: f"trusted={wait_for('the debug probe', lambda: probe('/health'))['trusted']}")

        def open_field():
            nonlocal pad_win, field
            field = subprocess.Popen([str(field_exe), str(scratch)])
            pad_win = find("oumu idle check")
            front(pad_win)
            # A plain Win32 edit box gives no word boxes through UI Automation, so it
            # gets no underlines; that Oumu reads it and finds the mistakes is enough here
            # (host_windows.py checks underline placement in Notepad).
            s = wait_for("Oumu to read the field and find mistakes", field_read)
            return f"{s['doc']['bundle_id']} read, {len(s['alerts'])} mistake(s) found"
        step("Text field is read", open_field)

        def idle():
            tk_procs.append(subprocess.Popen([sys.executable, "-c", TK, "Oumu plain check", "plain"]))
            front(find("Oumu plain check"))
            wait_for("the field to be let go", lambda: not (probe("/state").get("doc") or {}).get("active"), timeout=8)
            time.sleep(2.5)  # past the settle time after the last wake
            tps, calls, wakes = rate(6)
            assert tps <= 2.0, f"{tps:.1f} ticks/s with no field in front (want about 1; {wakes} wakes, {probe('/state')['status']})"
            return f"{tps:.1f} ticks/s, {calls} UI Automation calls, {wakes} wakes in 6 s"
        step("Idle: slow tick with no field in front", idle)

        def wake():
            plain = find("Oumu plain check")
            times = []
            for _ in range(5):
                front(plain)
                wait_for("idle", lambda: not (probe("/state").get("doc") or {}).get("active"), timeout=8)
                time.sleep(2.5)
                t0 = time.time()
                front(pad_win)
                wait_for("the field read again", field_read, timeout=5, every=0.02)
                times.append(time.time() - t0)
            worst = max(times)
            assert worst < 0.8, f"slowest wake {worst:.2f} s"
            return f"field read again in median {statistics.median(times):.2f} s, slowest {worst:.2f} s (5 tries)"
        step("Wake: field read again quickly on focus", wake)

        def fullscreen():
            front(pad_win)
            wait_for("the field read", field_read, timeout=5)
            before = [w for w in oumu_windows() if w["topmost"]]
            assert before, "no Oumu overlay visible before the fullscreen window"
            tk_procs.append(subprocess.Popen([sys.executable, "-c", TK, "Oumu full check", "full"]))
            front(find("Oumu full check"))
            wait_for("Oumu overlays hidden", lambda: not [w for w in oumu_windows() if w["topmost"]], timeout=4, every=0.1)
            time.sleep(2.5)
            tps, calls, _ = rate(6)
            still = [w for w in oumu_windows() if w["topmost"]]
            assert not still, f"overlays came back over the fullscreen window: {still}"
            assert tps <= 2.0, f"{tps:.1f} ticks/s with a fullscreen window in front"
            tk_procs.pop().kill()
            front(pad_win)
            wait_for("Oumu overlays back", lambda: [w for w in oumu_windows() if w["topmost"]], timeout=4, every=0.1)
            return f"{len(before)} overlay(s) hidden, {tps:.1f} ticks/s, {calls} UIA calls in 6 s; back after"
        step("Fullscreen: overlays hidden, loop slow", fullscreen)
    finally:
        for p in tk_procs + ([field] if field else []):
            p.kill()
        subprocess.run(["taskkill", "/F", "/T", "/PID", str(oumu.pid)], capture_output=True)
        try:
            scratch.unlink()
        except OSError:
            pass
        passed = sum(r["status"] == "PASS" for r in RESULTS)
        (out / "results.json").write_text(json.dumps(RESULTS, indent=1), encoding="utf-8")
        print(f"DONE {passed}/{len(RESULTS)} passed")
        sys.exit(0 if passed == len(RESULTS) else 1)


if __name__ == "__main__":
    main()
