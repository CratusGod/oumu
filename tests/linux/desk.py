"""Real-desktop helpers for the Linux end-to-end tests (Hyprland + ydotool).

Every pointer move and button press is a real input event from ydotool's uinput device,
so the compositor, the drag source and Utsuri all see what a user's mouse would make.
Nothing here types text: the only key it presses is the trigger key (Shift/Ctrl/Alt).
"""
from __future__ import annotations

import json
import os
import socket
import subprocess
import time
from pathlib import Path

RUN = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp"))
KEY_CODES = {"shift": 42, "ctrl": 29, "alt": 56, "esc": 1}
BTN_LEFT_DOWN, BTN_LEFT_UP = "0x40", "0x80"


def hypr(cmd: str) -> str:
    sig = os.environ["HYPRLAND_INSTANCE_SIGNATURE"]
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(2)
    s.connect(str(RUN / "hypr" / sig / ".socket.sock"))
    s.sendall(cmd.encode())
    out = b""
    while chunk := s.recv(65536):
        out += chunk
    s.close()
    return out.decode()


def hypr_json(what: str):
    return json.loads(hypr(f"j/{what}"))


def cursor() -> tuple[int, int]:
    x, y = hypr("cursorpos").split(",")
    return round(float(x)), round(float(y))


def ydotool(*args: str) -> None:
    subprocess.run(["ydotool", *args], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def ensure_ydotoold() -> None:
    # the socket file outlives a stopped service, so ask systemd, not the file
    active = subprocess.run(["systemctl", "--user", "is-active", "--quiet", "ydotool.service"]).returncode == 0
    if active and (RUN / ".ydotool_socket").exists():
        return
    subprocess.run(["systemctl", "--user", "start", "ydotool.service"], check=True)
    for _ in range(50):
        if (RUN / ".ydotool_socket").exists():
            return
        time.sleep(0.1)
    raise RuntimeError("ydotoold did not start")


def move_to(x: int, y: int, steps: int = 12, pause: float = 0.012) -> None:
    """Glide to (x, y) with relative moves (real motion events), correcting from the
    compositor's own idea of the pointer each step, so acceleration can't throw it off."""
    for i in range(steps, 0, -1):
        cx, cy = cursor()
        dx, dy = (x - cx) / i, (y - cy) / i
        if abs(dx) < 0.5 and abs(dy) < 0.5:
            break
        ydotool("mousemove", "-x", str(round(dx)), "-y", str(round(dy)))
        time.sleep(pause)
    for _ in range(4):  # last few px
        cx, cy = cursor()
        if (cx, cy) == (x, y):
            return
        ydotool("mousemove", "-x", str(x - cx), "-y", str(y - cy))
        time.sleep(pause)


def dispatch(lua: str) -> None:
    """One Hyprland dispatcher, Lua form (Hyprland 0.57's only form here)."""
    r = hypr(f"dispatch {lua}").strip()
    if r != "ok":
        raise RuntimeError(f"hyprland refused {lua}: {r}")


def jump_to(x: int, y: int) -> None:
    """Teleport (not a drag): the compositor's own cursor move."""
    dispatch(f"hl.dsp.cursor.move({{ x = {x}, y = {y} }})")
    time.sleep(0.05)


def button(down: bool) -> None:
    ydotool("click", BTN_LEFT_DOWN if down else BTN_LEFT_UP)


def key(name: str, down: bool) -> None:
    ydotool("key", f"{KEY_CODES[name]}:{1 if down else 0}")


def release_all() -> None:
    """Never leave a button or the trigger key stuck down."""
    for k in KEY_CODES:
        try:
            key(k, False)
        except Exception:
            pass
    try:
        button(False)
    except Exception:
        pass


def window(title: str | None = None, cls: str | None = None, pid: int | None = None):
    for c in hypr_json("clients"):
        if title and c["title"] != title:
            continue
        if cls and c["class"] != cls:
            continue
        if pid and c["pid"] != pid:
            continue
        return c
    return None


def wait_window(timeout: float = 15, **kw):
    end = time.time() + timeout
    while time.time() < end:
        if c := window(**kw):
            return c
        time.sleep(0.1)
    raise TimeoutError(f"no window {kw}")


def place(addr: str, x: int, y: int, w: int, h: int) -> None:
    """Float a window at an exact rect (runtime dispatches; no config is touched)."""
    sel = f'window = "address:{addr}"'
    dispatch(f'hl.dsp.window.float({{ action = "enable", {sel} }})')
    dispatch(f"hl.dsp.window.resize({{ x = {w}, y = {h}, exact = true, {sel} }})")
    dispatch(f"hl.dsp.window.move({{ x = {x}, y = {y}, exact = true, {sel} }})")
    time.sleep(0.15)


def active_window():
    return hypr_json("activewindow")


def workspace(name: str) -> None:
    dispatch(f'hl.dsp.focus({{ workspace = "{name}" }})')
    time.sleep(0.2)


def current_workspace() -> str:
    return hypr_json("activeworkspace")["name"]


def wait_for(pred, timeout: float, every: float = 0.05):
    end = time.time() + timeout
    while time.time() < end:
        v = pred()
        if v:
            return v
        time.sleep(every)
    return None
