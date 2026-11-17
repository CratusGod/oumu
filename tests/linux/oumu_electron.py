"""Oumu and an Electron app, started without and with --force-renderer-accessibility.

Without the flag Electron shares no text: Oumu must say so (status `hides_text`, a hint in the
panel). With it, Oumu must read the text area, find the mistakes and place underlines. Runs
on workspace 9 with its own Electron window (fixtures/electron_app) and Oumu's own data folder.

Run: python3 tests/linux/oumu_electron.py   (needs an Electron runtime, e.g. electron44)
"""
import sys, time, subprocess, json
from pathlib import Path
import shutil
HERE = Path(__file__).parent
sys.path.insert(0, str(HERE))
import oumu_probe as o, desk  # noqa: E402
APP = HERE / "fixtures" / "electron_app"
out = o.ROOT / "target" / "e2e" / f"oumu-electron-{int(time.time())}"
out.mkdir(parents=True, exist_ok=True)
ELECTRON = next((p for p in ("/usr/lib/electron44/electron", shutil.which("electron") or "") if p and Path(p).exists()), None)
if not ELECTRON:
    print("no Electron runtime found (install electron44)"); sys.exit(2)
results = []
def check(name, ok, detail=""):
    results.append((name, bool(ok))); print(("PASS" if ok else "FAIL"), name, "|", str(detail)[:150], flush=True)
def run_electron(flag):
    args = [ELECTRON, "--ozone-platform=wayland"] + (["--force-renderer-accessibility"] if flag else []) + [str(APP)]
    p = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    w = desk.wait_window(timeout=30, title="oumu-electron-test")
    desk.place(w["address"], 400, 200, 800, 450)
    desk.dispatch(f'hl.dsp.focus({{ window = "address:{w["address"]}" }})')
    return p, w
home = desk.current_workspace(); desk.workspace("9")
data = out / "data"; o.settings(data, False)
app = o.start(data)
try:
    check("oumu starts", o.wait(30, lambda: o.api("/health")["ok"]))
    o.wait(10, lambda: o.api("/health")["trusted"])
    # 1. without the flag: Oumu says the app hides its text
    e, w = run_electron(False)
    st = o.wait(15, lambda: (lambda s: s if s["status"].get("hides_text") else None)(o.api("/state")))
    s = st or o.api("/state")
    check("without the flag: Oumu sees the app hides its text", st, json.dumps(s["status"]))
    o.api("/panel", {"view": "review", "focus": False}); time.sleep(1.5)
    check("panel open: hint still on", o.api("/state")["status"].get("hides_text"), json.dumps(o.api("/state")["status"]))
    shot = out / "hint.png"; subprocess.run(["grim", str(shot)])
    log = (data / "oumu.log").read_text(errors="replace") if (data / "oumu.log").exists() else ""
    check("status log carries hides_text=true", "hides_text=true" in log)
    o.api("/panel", {"tucked": True})
    e.terminate(); e.wait(10); time.sleep(1)
    # 2. with the flag: Oumu reads the field and finds the mistakes
    e, w = run_electron(True)
    st = o.wait(25, lambda: (lambda s: s if s.get("doc") and "sentense" in (s["doc"] or {}).get("text", "") and len(s.get("alerts", [])) >= 2 else None)(o.api("/state")))
    s = st or o.api("/state")
    check("with the flag: Oumu reads the Electron text field", s.get("doc") and "sentense" in (s["doc"] or {}).get("text", ""), (s.get("doc") or {}).get("text", "")[:80])
    check("with the flag: mistakes found", len(s.get("alerts", [])) >= 2, ", ".join(a["original"] for a in s.get("alerts", [])))
    check("with the flag: no hint", not s["status"].get("hides_text"), json.dumps(s["status"]))
    g = o.wait(15, lambda: (lambda x: x if len(x.get("geometry", {}).get("rects", [])) >= 2 else None)(o.api("/state")))
    gs = (g or o.api("/state")).get("geometry", {})
    check("underlines placed", g, json.dumps({k: gs.get(k) for k in ("origin_x", "origin_y", "width", "height")}) + f' rects={len(gs.get("rects", []))}')
    e.terminate(); e.wait(10)
finally:
    app.terminate(); app.wait(10); desk.workspace(home)
n = sum(ok for _, ok in results); print(f"{n}/{len(results)} passed"); sys.exit(0 if n == len(results) else 1)
