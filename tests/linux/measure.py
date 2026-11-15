"""Measure Utsuri and Oumu on this desktop: start-up time, idle and busy CPU, RAM, GPU memory,
response times and what they store on disk.

Every app here is a test instance with its own data folder; the user's running apps are only
read (from /proc), never touched.

  python3 tests/linux/measure.py            idle + start-up + speed + storage
  python3 tests/linux/measure.py --busy     also sample both apps through their E2E runs
"""
from __future__ import annotations

import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

HERE = Path(__file__).parent
sys.path.insert(0, str(HERE))
import oumu_probe as o  # noqa: E402

ROOT = HERE.parents[1]
REL = ROOT / "target" / "release"
TICK = os.sysconf("SC_CLK_TCK")
IDLE_S = 60
STARTS = 5


# ------------------------------------------------------------------ process sampling

def cpu_ticks(pid: int) -> int:
    f = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return int(f[11]) + int(f[12])  # utime + stime


def mem_kb(pid: int) -> dict:
    """Rss and Pss. A process started through newgrp (Utsuri before a re-login) isn't
    dumpable, so its smaps_rollup can't be read: then Rss comes from status, Pss is absent."""
    out = {}
    try:
        text = Path(f"/proc/{pid}/smaps_rollup").read_text()
    except PermissionError:
        text = Path(f"/proc/{pid}/status").read_text().replace("VmRSS", "Rss")
    for line in text.splitlines():
        m = re.match(r"(Rss|Pss|Private_Dirty|Swap):\s+(\d+) kB", line)
        if m:
            out[m[1]] = int(m[2])
    return out


def threads(pid: int) -> int:
    return len(os.listdir(f"/proc/{pid}/task"))


def vram_mib() -> dict[int, int]:
    """GPU memory per pid (NVIDIA); empty when the driver doesn't report it."""
    # graphics clients (Vulkan windows) only show in the full listing, not in --query-*
    try:
        full = subprocess.run(["nvidia-smi"], capture_output=True, text=True, timeout=10).stdout
    except (OSError, subprocess.TimeoutExpired):
        return {}
    return {int(m[1]): int(m[2]) for m in re.finditer(r"\s(\d+)\s+[CG+]+\s+\S.*?\s(\d+)MiB", full)}


class Sampler:
    """CPU % (of one core) every second, plus memory, for one pid."""

    def __init__(self, pid: int) -> None:
        self.pid, self.cpu, self.rss, self.pss = pid, [], [], []
        self.stop = threading.Event()
        self.t = threading.Thread(target=self.run, daemon=True)

    def run(self) -> None:
        last, lt = cpu_ticks(self.pid), time.time()
        while not self.stop.wait(1.0):
            try:
                now, nt = cpu_ticks(self.pid), time.time()
                m = mem_kb(self.pid)
            except (OSError, IndexError):
                return
            self.cpu.append((now - last) / TICK / (nt - lt) * 100)
            last, lt = now, nt
            self.rss.append(m.get("Rss", 0))
            self.pss.append(m.get("Pss", 0))

    def __enter__(self):
        self.t.start()
        return self

    def __exit__(self, *_):
        self.stop.set()
        self.t.join()

    def summary(self) -> dict:
        c = self.cpu or [0.0]
        return {
            "cpu_avg_pct": round(statistics.mean(c), 2),
            "cpu_p95_pct": round(sorted(c)[int(len(c) * 0.95) - 1 if len(c) > 1 else 0], 1),
            "cpu_max_pct": round(max(c), 1),
            "rss_mb_end": round((self.rss or [0])[-1] / 1024, 1),
            "rss_mb_max": round(max(self.rss or [0]) / 1024, 1),
            "pss_mb_end": round((self.pss or [0])[-1] / 1024, 1),
            "samples": len(self.cpu),
        }


def find_pid(pattern: str, timeout: float = 30) -> int | None:
    end = time.time() + timeout
    while time.time() < end:
        r = subprocess.run(["pgrep", "-n", "-f", pattern], capture_output=True, text=True)
        if r.stdout.strip():
            return int(r.stdout.split()[0])
        time.sleep(0.2)
    return None


# ------------------------------------------------------------------ the apps

def utsuri_env(tmp: Path) -> dict:
    e = dict(os.environ)
    e.update(XDG_CONFIG_HOME=str(tmp / "config"), XDG_STATE_HOME=str(tmp / "state"), UTSURI_PIPE=str(tmp / "utsuri.sock"))
    return e


def start_utsuri(tmp: Path) -> tuple[subprocess.Popen, float]:
    sock = tmp / "utsuri.sock"
    t = time.time()
    p = subprocess.Popen([str(REL / "utsuri")], env=utsuri_env(tmp), stdout=subprocess.DEVNULL, stderr=open(tmp / "stderr.txt", "ab"))
    while not sock.exists() and time.time() - t < 30:
        time.sleep(0.005)
    return p, time.time() - t


def start_oumu(data: Path) -> tuple[subprocess.Popen, float]:
    o.EXE = REL / "oumu"
    t = time.time()
    p = o.start(data)
    o.wait(30, lambda: o.api("/health", timeout=2)["ok"], every=0.005)
    return p, time.time() - t


def stop(p: subprocess.Popen) -> None:
    p.terminate()
    try:
        p.wait(10)
    except subprocess.TimeoutExpired:
        p.kill()


def startup_times() -> dict:
    out = {}
    with tempfile.TemporaryDirectory() as d:
        ts = []
        for i in range(STARTS):
            tmp = Path(d) / f"u{i}"
            tmp.mkdir()
            p, s = start_utsuri(tmp)
            ts.append(s)
            stop(p)
            time.sleep(0.5)
        out["utsuri"] = ts
        ts = []
        for i in range(STARTS):
            data = Path(d) / f"o{i}"
            o.settings(data, False)
            p, s = start_oumu(data)
            ts.append(s)
            stop(p)
            time.sleep(0.5)
        out["oumu"] = ts
    return {k: {"median_ms": round(statistics.median(v) * 1000), "min_ms": round(min(v) * 1000), "max_ms": round(max(v) * 1000)} for k, v in out.items()}


def idle(name: str) -> dict:
    with tempfile.TemporaryDirectory() as d:
        tmp = Path(d)
        if name == "utsuri":
            p, _ = start_utsuri(tmp)
        else:
            o.settings(tmp / "data", False)
            p, _ = start_oumu(tmp / "data")
        time.sleep(8)  # past start-up work
        with Sampler(p.pid) as s:
            time.sleep(IDLE_S)
        r = s.summary() | {"threads": threads(p.pid), "vram_mib": vram_mib().get(p.pid)}
        stop(p)
        return r


def oumu_speed() -> dict:
    """Response times of the offline paths, and opening and tucking the panel."""
    with tempfile.TemporaryDirectory() as d:
        data = Path(d) / "data"
        o.settings(data, False)
        p, _ = start_oumu(data)
        text = "Their is a mistake in this sentense. She dont like apples, and me and him goes to the store yesterday. " * 4
        res = {}
        for name, args in (("score", None), ("rewriter_scan", {"text": text}), ("spot_check", {"text": text})):
            if args is None:
                continue
            ts = []
            for _ in range(5):
                t = time.time()
                o.api("/feature", {"name": name, "args": args})
                ts.append((time.time() - t) * 1000)
            res[f"{name}_ms"] = round(statistics.median(ts), 1)
        ts = []
        for _ in range(5):
            t = time.time()
            o.api("/panel", {"view": "review", "focus": False})
            # visible to Hyprland
            while not any(c["title"] == "Oumu" and c["pid"] == p.pid for c in json.loads(subprocess.run(["hyprctl", "-j", "clients"], capture_output=True, text=True).stdout)):
                time.sleep(0.003)
            ts.append((time.time() - t) * 1000)
            time.sleep(0.6)
            o.api("/panel", {"tucked": True})
            time.sleep(0.8)
        res["panel_open_to_window_ms"] = round(statistics.median(ts))
        stop(p)
        return res


def disk(path: Path) -> int:
    if not path.exists():
        return 0
    if path.is_file():
        return path.stat().st_size
    return sum(f.stat().st_size for f in path.rglob("*") if f.is_file())


def storage() -> dict:
    h = Path.home()
    mb = lambda b: round(b / 1e6, 1)  # noqa: E731
    return {
        "utsuri binary (installed)": mb(disk(h / ".local/bin/utsuri")),
        "oumu binary (installed)": mb(disk(h / ".local/bin/oumu")),
        "icons, app entries, service menu": mb(sum(disk(p) for p in [*(h / ".local/share/icons").rglob("dev.cratusgod.*"), *(h / ".local/share/applications").glob("dev.cratusgod.*"), h / ".local/share/kio/servicemenus/utsuri.desktop"])),
        "oumu data (settings, history, dictionary)": mb(disk(h / ".local/share/dev.cratusgod.oumu")),
        "oumu log": mb(disk(h / ".local/state/oumu")),
        "utsuri settings": mb(disk(h / ".config/Utsuri-rs")),
        "utsuri log + ledger": mb(disk(h / ".local/state/Utsuri-rs")),
    }


def users_running() -> dict:
    """The user's own running apps (read only)."""
    out = {}
    for name in ("oumu", "utsuri"):
        r = subprocess.run(["pgrep", "-x", name], capture_output=True, text=True)
        pids = [int(x) for x in r.stdout.split()]
        if not pids:
            continue
        pid = pids[0]
        m = mem_kb(pid)
        c0, t0 = cpu_ticks(pid), time.time()
        time.sleep(10)
        c1, t1 = cpu_ticks(pid), time.time()
        up = time.time() - Path(f"/proc/{pid}").stat().st_mtime
        out[name] = {"rss_mb": round(m["Rss"] / 1024, 1), "pss_mb": round(m["Pss"] / 1024, 1), "cpu_pct_10s": round((c1 - c0) / TICK / (t1 - t0) * 100, 2), "uptime_min": round(up / 60), "threads": threads(pid), "vram_mib": vram_mib().get(pid)}
    return out


def busy(cmd: list[str], pattern: str, env: dict | None = None) -> dict:
    """Sample every process matching `pattern` while a test run goes (a run may restart the
    app between sections): CPU summed across them, memory of the live one."""
    run = subprocess.Popen(cmd, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    out_lines: list[str] = []
    reader = threading.Thread(target=lambda: out_lines.extend(run.stdout or []), daemon=True)
    reader.start()
    last: dict[int, int] = {}
    cpu, rss, pss, apps = [], [], [], set()
    peak_vram = 0
    lt = time.time()
    while run.poll() is None:
        time.sleep(1.0)
        r = subprocess.run(["pgrep", "-f", pattern], capture_output=True, text=True)
        pids = [int(x) for x in r.stdout.split()]
        now, used, mem = time.time(), 0, {"Rss": 0, "Pss": 0}
        for pid in pids:
            try:
                t = cpu_ticks(pid)
                m = mem_kb(pid)
            except (OSError, IndexError):
                continue
            used += t - last.get(pid, t)
            last[pid] = t
            mem = {k: mem[k] + m.get(k, 0) for k in mem}
            apps.add(pid)
        if pids:
            cpu.append(used / TICK / (now - lt) * 100)
            rss.append(mem["Rss"])
            pss.append(mem["Pss"])
            v = vram_mib()
            peak_vram = max([peak_vram] + [v.get(pid, 0) for pid in pids])
        lt = now
    reader.join(5)
    c = cpu or [0.0]
    tail = [l.strip() for l in out_lines if l.strip()][-1:]
    return {
        "cpu_avg_pct": round(statistics.mean(c), 2), "cpu_p95_pct": round(sorted(c)[max(0, int(len(c) * 0.95) - 1)], 1), "cpu_max_pct": round(max(c), 1),
        "rss_mb_max": round(max(rss or [0]) / 1024, 1), "pss_mb_max": round(max(pss or [0]) / 1024, 1), "samples": len(cpu), "app_starts": len(apps),
        "vram_mib_peak": peak_vram or None, "result": tail[0] if tail else "",
    }


def main() -> int:
    report: dict = {}
    print("storage …", flush=True)
    report["storage_mb"] = storage()
    print("your running apps …", flush=True)
    report["your_running_apps"] = users_running()
    print("start-up …", flush=True)
    report["startup"] = startup_times()
    print("speed …", flush=True)
    report["oumu_speed"] = oumu_speed()
    for name in ("utsuri", "oumu"):
        print(f"{name} idle ({IDLE_S} s) …", flush=True)
        report[f"{name}_idle"] = idle(name)
    if "--busy" in sys.argv:
        env = dict(os.environ, CRATUS_PROFILE="release")
        print("utsuri busy (E2E, 65 drags) …", flush=True)
        report["utsuri_busy"] = busy(["cargo", "run", "--release", "-q", "-p", "utsuri-e2e"], "target/release/utsuri( |$)", env)
        print("oumu busy (E2E with AI) …", flush=True)
        report["oumu_busy"] = busy([sys.executable, str(HERE / "oumu_e2e.py")], "target/release/oumu$", env)
    out = ROOT / "target" / "e2e" / "measure-latest.json"
    out.write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
