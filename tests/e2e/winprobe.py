"""Small helpers for driving Oumu's debug probe from the Windows app checks."""
from __future__ import annotations

import json
import time
import urllib.request

PORT = 7811


def probe(path: str, body: dict | None = None, timeout: float = 15) -> dict:
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(f"http://127.0.0.1:{PORT}{path}", data=data, method="POST" if data else "GET")
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.load(r)


def wait_for(what: str, fn, timeout: float = 25, every: float = 0.4):
    end = time.time() + timeout
    last = None
    while time.time() < end:
        try:
            last = fn()
            if last:
                return last
        except Exception as e:  # the probe is not up yet, etc.
            last = e
        time.sleep(every)
    raise AssertionError(f"timed out waiting for {what} (last: {str(last)[:300]})")


def oumu_windows() -> list[dict]:
    """Oumu's visible top-level windows: rect, size, click-through, topmost."""
    import ctypes
    import ctypes.wintypes as wt
    import subprocess
    user32 = ctypes.windll.user32
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq oumu.exe", "/FO", "CSV", "/NH"],
                         capture_output=True, text=True).stdout
    pids = {int(ln.split(",")[1].strip('"')) for ln in out.splitlines() if ln.startswith('"oumu')}
    found = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, wt.HWND, wt.LPARAM)
    def each(h, _):
        pid = wt.DWORD()
        user32.GetWindowThreadProcessId(h, ctypes.byref(pid))
        if pid.value in pids and user32.IsWindowVisible(h):
            r = wt.RECT()
            user32.GetWindowRect(h, ctypes.byref(r))
            ex = user32.GetWindowLongW(h, -20)
            found.append({"x": r.left, "y": r.top, "w": r.right - r.left, "h": r.bottom - r.top,
                          "click_through": bool(ex & 0x20), "topmost": bool(ex & 0x8)})
        return True
    user32.EnumWindows(each, 0)
    return found


def summary() -> str:
    s = probe("/state")
    d = s.get("doc") or {}
    g = s.get("geometry") or {}
    return json.dumps({
        "front": (s.get("status") or {}).get("frontmost_bid"),
        "doc_app": d.get("bundle_id"), "role": d.get("role"), "text": (d.get("text") or "")[:90],
        "sel": (d.get("sel_start"), d.get("sel_end")), "frame": d.get("frame"), "visible": d.get("visible"),
        "alerts": [(a["original"], (a["suggestions"] or ["?"])[0]) for a in (s.get("alerts") or [])][:6],
        "rects": [(round(r["x"]), round(r["y"]), round(r["w"]), round(r["h"])) for r in (g.get("rects") or [])][:6],
        "origin": (g.get("origin_x"), g.get("origin_y")),
    }, ensure_ascii=False)
