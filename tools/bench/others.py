"""DDM against other downloaders, black-box from outside: the lab server's scenarios and real servers.

    python tools/bench/others.py detect
    python tools/bench/others.py lab [--runs N] [--only fast-1g,...] [--clients ddm,curl,aria2c] [--out DIR]
    python tools/bench/others.py internet [--seconds 15] [--url URL ...] [--clients ...] [--out DIR]
    python tools/bench/others.py report DIR

Each client is driven the way a person would script it, never from inside:
  ddm      DDM's command line: `dannexdownloadmanager --cli --json get URL -o FILE -c 8` (its own
           data folder, MDM_DATA_DIR, so the user's copy is untouched); `ddm-16`: 16 connections;
           `ddm-before`: another build of DDM (--ddm-before), to time a change against the build
           before it on the same runs.
  curl     one connection: `curl -sS -o FILE URL`.
  aria2c   aria2's own RPC (aria2c --enable-rpc, a private instance on 127.0.0.1): addUri with
           split=16, max-connection-per-server=16, min-split-size=1M (aria2's default 20M would
           cap a 100 MiB file at 5 connections); `aria2c-8`: 8 of each.
  motrix   Motrix is aria2 inside: the same RPC on the port and secret its settings show
           (--motrix-rpc, --motrix-secret). Only when it is running and answers; never started or
           installed by this script.
  fdm      Free Download Manager has no documented command line or API that starts a download
           into a given folder and says when it is done, so it can't be timed from outside: it is
           detected and skipped, with the note in the report.
Download managers that aren't installed are skipped with a note, never installed.

`lab`: every scenario of bench.py on the lab server (one server, faults per URL), clients in a
rotating order per run, one warm-up run per client and scenario discarded. A run's time is end to
end (the command issued -> the file complete: the process's exit, or aria2's status `complete`),
checked against the expected size; the lab server's log gives each run's phases (startup:
command -> first request; transfer: first request -> last byte sent; finish: last byte -> done),
connections (distinct TCP ports), requests and bytes sent over the file size.

`internet`: each client downloads each real address for at most --seconds, then is stopped; the
result is the bytes it had after 2 s, 5 s and at the end, and its average speed. Clients take
turns per address, so a line that changes speed over minutes hits them alike. Keep it short: these
are other people's servers.

Raw results go to DIR/runs.json (lab) or DIR/internet.json, the machine to DIR/environment.json;
`report` writes DIR/REPORT.md. A run published in docs/bench/ keeps its JSON files there, and its
report is folded into the dated speed write-up (docs/bench/speed-<date>.md, an appendix of it, as
docs/bench/others-2026-11-27/'s are) rather than committed as a REPORT.md of its own.
"""

import argparse
import datetime as dt
import json
import os
import platform
import shutil
import subprocess
import sys
import threading
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).parent))

PORT = 8098
EXE = ".exe" if os.name == "nt" else ""

# The lab scenarios: bench.py's (IDM vs DDM), plus those only a command-line client can run in
# reasonable time.  id, title, size, faults, runs, timeout s
EXTRA_SCENARIOS = [
    ("stall-32m", "32 MiB, every 3rd response goes silent after 1 MiB, 4 MB/s", "32M",
     "rate=4M&stall_every=3&stall_after=1M", 5, 300),
    ("highlat-50m", "50 MiB, 300 ms latency, 1 MB/s per connection", "50M",
     "rate=1M&latency_ms=300", 5, 300),
    ("slowanswer-1m", "1.3 MiB, every answer 1.3 s late (a CDN slow to answer), 4 MB/s", "1300K",
     "rate=4M&latency_ms=1300", 5, 120),
]

# Real servers, for `internet` (big files; each client stops after --seconds). Tele2's
# speedtest.tele2.net is left out: unreachable from some networks (2026-11-27: its TCP connections
# timed out on the measuring line), which says nothing about the downloaders.
# Cloudflare's speed test answers 403 above 100 MB a request and serves no ranges (one connection
# for everyone), so it is asked for 25 MB. proof.ovh.net answers 429 beyond a few connections from
# one address and stops answering it for minutes after a burst: keep it last, run it once.
INTERNET_URLS = [
    "https://dl.google.com/chrome/install/googlechromestandaloneenterprise64.msi",
    "https://github.com/git-for-windows/git/releases/download/v2.47.1.windows.1/Git-2.47.1-64-bit.exe",
    "https://fsn1-speed.hetzner.com/100MB.bin",
    "https://ash-speed.hetzner.com/100MB.bin",
    "https://speed.cloudflare.com/__down?bytes=25000000",
    "https://proof.ovh.net/files/100Mb.dat",
]

# Runs that can't say anything: (scenario, client) -> why, in the report.
SKIP = {
    ("stall-32m", "curl"): "curl has one connection and no stall recovery of its own: a response "
                           "that goes silent hangs it until it is killed",
}


def scenarios():
    from bench import SCENARIOS  # noqa: PLC0415 (bench.py is Windows-only: imported when needed)
    return SCENARIOS + EXTRA_SCENARIOS


def scenarios_portable():
    """bench.py's scenarios without importing it (it loads Win32 at import)."""
    if os.name == "nt":
        return scenarios()
    import ast
    tree = ast.parse((Path(__file__).parent / "bench.py").read_text(encoding="utf-8"))
    for node in tree.body:
        if isinstance(node, ast.Assign) and getattr(node.targets[0], "id", "") == "SCENARIOS":
            return ast.literal_eval(node.value) + EXTRA_SCENARIOS
    return EXTRA_SCENARIOS


def size_bytes(s):
    n, u = s[:-1], s[-1].upper()
    return int(float(n) * {"K": 2**10, "M": 2**20, "G": 2**30}[u])


# ---- the process: CPU time and peak memory ----------------------------------------------------
if os.name == "nt":
    import ctypes
    import ctypes.wintypes as wt

    _k32 = ctypes.windll.kernel32
    _k32.OpenProcess.restype = wt.HANDLE

    class _PMC(ctypes.Structure):
        _fields_ = [("cb", wt.DWORD), ("PageFaultCount", wt.DWORD), ("PeakWorkingSetSize", ctypes.c_size_t),
                    ("WorkingSetSize", ctypes.c_size_t), ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaPagedPoolUsage", ctypes.c_size_t), ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaNonPagedPoolUsage", ctypes.c_size_t), ("PagefileUsage", ctypes.c_size_t),
                    ("PeakPagefileUsage", ctypes.c_size_t)]

    def process_usage(pid):
        """(CPU seconds, peak working set MB) of a process, read while its handle is open."""
        h = _k32.OpenProcess(0x1000 | 0x0010, False, pid)
        if not h:
            return None
        return h

    def read_usage(h):
        c, e, k, u = wt.FILETIME(), wt.FILETIME(), wt.FILETIME(), wt.FILETIME()
        _k32.GetProcessTimes(h, ctypes.byref(c), ctypes.byref(e), ctypes.byref(k), ctypes.byref(u))
        t = lambda f: (f.dwHighDateTime << 32 | f.dwLowDateTime) / 1e7  # noqa: E731
        m = _PMC()
        m.cb = ctypes.sizeof(_PMC)
        ctypes.windll.psapi.GetProcessMemoryInfo(h, ctypes.byref(m), m.cb)
        return t(k) + t(u), m.PeakWorkingSetSize / 2**20
else:
    def process_usage(pid):
        return None

    def read_usage(h):
        return None, None


# ---- clients -------------------------------------------------------------------------------------
def target_dir():
    return Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))


def find_ddm(given):
    for p in [given, os.environ.get("DDM_EXE"), target_dir() / "release" / f"dannexdownloadmanager{EXE}"]:
        if p and Path(p).is_file():
            return Path(p)
    return None


def find_labserver(given):
    for p in [given, target_dir() / "release" / f"labserver{EXE}"]:
        if p and Path(p).is_file():
            return Path(p)
    return None


def fdm_paths():
    if os.name != "nt":
        return [Path("/opt/freedownloadmanager/fdm"), Path("/Applications/Free Download Manager.app")]
    pf = [os.environ.get("ProgramFiles", r"C:\Program Files"), os.environ.get("ProgramFiles(x86)", "")]
    return [Path(b) / "Softdeluxe" / "Free Download Manager" / "fdm.exe" for b in pf if b] + \
           [Path(b) / "Free Download Manager" / "fdm.exe" for b in pf if b]


def motrix_paths():
    if os.name != "nt":
        return [Path("/Applications/Motrix.app"), Path("/opt/Motrix/motrix")]
    local = os.environ.get("LOCALAPPDATA", "")
    pf = os.environ.get("ProgramFiles", r"C:\Program Files")
    return [Path(local) / "Programs" / "Motrix" / "Motrix.exe", Path(pf) / "Motrix" / "Motrix.exe"]


def aria2_rpc(url, secret, method, *params, timeout=10):
    body = json.dumps({"jsonrpc": "2.0", "id": "bench", "method": method,
                       "params": ([f"token:{secret}"] if secret else []) + list(params)}).encode()
    req = urllib.request.Request(url, body, {"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        out = json.loads(r.read())
    if "error" in out:
        raise RuntimeError(out["error"])
    return out["result"]


class Aria2:
    """An aria2 JSON-RPC endpoint: a private aria2c started here, or Motrix's."""

    def __init__(self, name, url, secret, proc=None, version=None):
        self.name, self.url, self.secret, self.proc, self.version = name, url, secret, proc, version

    @classmethod
    def start_private(cls, exe, port=6899):
        secret = "ddm-bench"
        proc = subprocess.Popen([str(exe), "--enable-rpc", f"--rpc-listen-port={port}", f"--rpc-secret={secret}",
                                 "--rpc-listen-all=false", "--quiet=true", "--check-certificate=true",
                                 "--file-allocation=none", "--auto-file-renaming=false", "--allow-overwrite=true",
                                 "--max-concurrent-downloads=1", "--save-session-interval=0",
                                 "--disk-cache=16M", "--summary-interval=0"],
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        url = f"http://127.0.0.1:{port}/jsonrpc"
        for _ in range(100):
            try:
                v = aria2_rpc(url, secret, "aria2.getVersion", timeout=1)
                return cls("aria2c", url, secret, proc, v.get("version"))
            except OSError:
                time.sleep(0.05)
        proc.kill()
        raise RuntimeError("aria2c's RPC did not come up")

    def add(self, uri, dest, connections):
        opts = {"dir": str(dest.parent), "out": dest.name, "split": str(connections),
                "max-connection-per-server": str(connections), "min-split-size": "1M",
                "allow-overwrite": "true", "auto-file-renaming": "false", "file-allocation": "none"}
        return aria2_rpc(self.url, self.secret, "aria2.addUri", [uri], opts)

    def status(self, gid):
        """aria2's status of `gid`; None when it doesn't answer in time (busy writing to the disk:
        it answers from the same thread that writes)."""
        try:
            return aria2_rpc(self.url, self.secret, "aria2.tellStatus", gid,
                             ["status", "completedLength", "totalLength", "connections", "errorMessage"],
                             timeout=60)
        except TimeoutError:
            return None

    def remove(self, gid):
        for m in ("aria2.forceRemove", "aria2.removeDownloadResult"):
            try:
                aria2_rpc(self.url, self.secret, m, gid)
            except (OSError, RuntimeError):
                pass

    def stop(self):
        if self.proc:
            try:
                aria2_rpc(self.url, self.secret, "aria2.shutdown", timeout=2)
                self.proc.wait(5)
            except (OSError, RuntimeError, subprocess.TimeoutExpired):
                self.proc.kill()


class Clients:
    """What is installed, how to run it, and the notes for what isn't."""

    def __init__(self, args, wanted):
        self.notes = {}
        self.ddm = find_ddm(args.ddm)
        self.ddm_before = Path(args.ddm_before) if args.ddm_before else None
        self.curl = shutil.which(args.curl or "curl")
        self.aria2c = args.aria2c or shutil.which("aria2c")
        self.data = Path(args.out) / "ddm-data"
        self.aria2 = None
        self.motrix = None
        self.wanted = []
        for c in wanted:
            if c == "ddm-before":
                if self.ddm_before and self.ddm_before.is_file():
                    self.wanted.append(c)
                else:
                    self.notes[c] = "no build before the change given (--ddm-before PATH)"
            elif c.startswith("ddm"):
                if self.ddm:
                    self.wanted.append(c)
                else:
                    self.notes[c] = "DDM's program isn't built (cargo build --release -p mdm-app) or given (--ddm)"
            elif c == "curl":
                (self.wanted.append(c) if self.curl else self.notes.__setitem__(c, "curl isn't on PATH"))
            elif c.startswith("aria2c"):
                if self.aria2c and Path(self.aria2c).is_file():
                    self.wanted.append(c)
                else:
                    self.notes[c] = ("aria2c isn't on PATH: download aria2's release from "
                                     "github.com/aria2/aria2/releases and give --aria2c PATH")
            elif c == "motrix":
                installed = [p for p in motrix_paths() if p.exists()]
                try:
                    v = aria2_rpc(args.motrix_rpc, args.motrix_secret, "aria2.getVersion", timeout=2)
                    self.motrix = Aria2("motrix", args.motrix_rpc, args.motrix_secret, None, v.get("version"))
                    self.wanted.append(c)
                except (OSError, RuntimeError) as e:
                    self.notes[c] = (f"Motrix {'is installed but its' if installed else 'is not installed; its'} "
                                     f"aria2 RPC at {args.motrix_rpc} did not answer ({e.__class__.__name__}). "
                                     "Start Motrix, read Settings -> Advanced -> RPC (port, secret) and give "
                                     "--motrix-rpc http://127.0.0.1:PORT/jsonrpc --motrix-secret SECRET")
            elif c == "fdm":
                installed = [p for p in fdm_paths() if p.exists()]
                self.notes[c] = (("Free Download Manager is installed (" + str(installed[0]) + ") but"
                                  if installed else "Free Download Manager is not installed, and")
                                 + " it has no documented command line or API that starts a download into a "
                                   "given folder and reports completion, so it can't be timed from outside. "
                                   "To compare by hand: add each scenario's address in FDM with the same "
                                   "folder and time it against `lab`'s server log (python tools/bench/others.py "
                                   "lab --clients ddm keeps the server's log in DIR/server.jsonl).")
            else:
                self.notes[c] = "unknown client"

    def versions(self):
        v = {}
        if self.ddm:
            head = subprocess.run(["git", "-C", str(ROOT), "rev-parse", "--short", "HEAD"],
                                  capture_output=True, text=True).stdout.strip()
            dirty = subprocess.run(["git", "-C", str(ROOT), "status", "--porcelain", "--", "crates"],
                                   capture_output=True, text=True).stdout.strip()
            v["ddm"] = f"{self.ddm} ({head}{' with uncommitted changes' if dirty else ''})"
        if "ddm-before" in self.wanted:
            v["ddm-before"] = str(self.ddm_before)
        if self.curl:
            v["curl"] = subprocess.run([self.curl, "--version"], capture_output=True, text=True).stdout.split("\n")[0]
        if self.aria2c and any(c.startswith("aria2c") for c in self.wanted):
            v["aria2c"] = subprocess.run([self.aria2c, "--version"], capture_output=True,
                                         text=True).stdout.split("\n")[0]
        if self.motrix:
            v["motrix"] = f"aria2 {self.motrix.version} (Motrix's)"
        return v

    def start(self):
        if any(c.startswith("aria2c") for c in self.wanted):
            self.aria2 = Aria2.start_private(self.aria2c)

    def stop(self):
        if self.aria2:
            self.aria2.stop()

    def connections(self, client):
        if client.endswith("-16") or client == "aria2c":
            return 16
        if client == "curl":
            return 1
        return 8

    def endpoint(self, client):
        return self.motrix if client == "motrix" else self.aria2

    def command(self, client, url, dest):
        if client.startswith("ddm"):
            exe = self.ddm_before if client == "ddm-before" else self.ddm
            return [str(exe), "--cli", "--json", "get", url, "-o", str(dest),
                    "-c", str(self.connections(client))]
        if client == "curl":
            return [self.curl, "-sS", "--fail", "-L", "-o", str(dest), url]
        return None

    def env(self, client):
        if client.startswith("ddm"):
            self.data.mkdir(parents=True, exist_ok=True)
            return dict(os.environ, MDM_DATA_DIR=str(self.data), DDM_DATA_DIR=str(self.data))
        return None


def clean(dest):
    for p in (dest, Path(str(dest) + ".ddm.part"), Path(str(dest) + ".ddm.json"),
              Path(str(dest) + ".ddm.pieces"), Path(str(dest) + ".aria2")):
        try:
            p.unlink()
        except OSError:
            pass


def run_once(clients, client, url, dest, timeout, expected=None, stop_after=None):
    """One download. Lab: until complete (or `timeout`). Internet: stopped after `stop_after` s.
    Returns the run's record with a timeline of (seconds, bytes) where the client tells it."""
    clean(dest)
    timeline = []
    usage = (None, None)
    t0 = time.perf_counter()
    ok, err = False, None
    if clients.endpoint(client) is not None and not client.startswith(("ddm", "curl")):
        ep = clients.endpoint(client)
        gid = ep.add(url, dest, clients.connections(client))
        conns = []
        while True:
            now = time.perf_counter() - t0
            s = ep.status(gid)
            if s is None:
                continue
            got = int(s.get("completedLength", 0))
            timeline.append((round(now, 3), got))
            conns.append(int(s.get("connections", 0)))
            if s["status"] == "complete":
                ok = True
                break
            if s["status"] in ("error", "removed"):
                err = s.get("errorMessage", s["status"])
                break
            if (stop_after and now >= stop_after) or now >= timeout:
                err = "stopped" if stop_after else "timeout"
                break
            time.sleep(0.02 if expected else 0.25)
        done = time.perf_counter()
        ep.remove(gid)
        peak_conns = max(conns) if conns else None
    else:
        cmd = clients.command(client, url, dest)
        proc = subprocess.Popen(cmd, env=clients.env(client), stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="replace")
        h = process_usage(proc.pid)
        peak_conns = None
        if client.startswith("ddm"):
            def read():
                nonlocal peak_conns
                for line in proc.stdout:
                    try:
                        e = json.loads(line)
                    except ValueError:
                        continue
                    if e.get("event") == "progress":
                        timeline.append((round(time.perf_counter() - t0, 3), int(e.get("downloaded", 0))))
                        peak_conns = max(peak_conns or 0, int(e.get("connections", 0)))
            reader = threading.Thread(target=read, daemon=True)
            reader.start()
        limit = stop_after or timeout
        while proc.poll() is None and time.perf_counter() - t0 < limit:
            if h:
                usage = read_usage(h)
            time.sleep(0.001 if expected else 0.05)
        done = time.perf_counter()
        if h:
            usage = read_usage(h)
        if proc.poll() is None:
            proc.kill()
            err = "stopped" if stop_after else "timeout"
        else:
            ok = proc.returncode == 0
            if not ok:
                err = (proc.stderr.read() or "").strip()[-300:] or f"exit {proc.returncode}"
        proc.wait()
        if client == "curl":
            # curl says nothing while it runs: what it wrote is what it had.
            timeline.append((round(min(done - t0, stop_after or timeout), 3),
                             dest.stat().st_size if dest.exists() else 0))
    size = dest.stat().st_size if dest.exists() else None
    rec = {
        "client": client,
        "ok": ok and (expected is None or size == expected),
        "error": err,
        "e2e_ms": round((done - t0) * 1000, 2),
        "t0_ms": round(t0 * 1000, 3),
        "done_ms": round(done * 1000, 3),
        "file_bytes": size,
        "cpu_ms": round(usage[0] * 1000, 1) if usage[0] is not None else None,
        "peak_ws_mb": round(usage[1], 1) if usage[1] is not None else None,
        "peak_connections": peak_conns,
        "timeline": timeline,
    }
    return rec


def environment(clients, extra):
    env = {
        "date": dt.datetime.now().isoformat(timespec="seconds"),
        "os": platform.platform(),
        "cpu": platform.processor(),
        "clients": clients.versions(),
        "skipped": clients.notes,
    }
    if os.name == "nt":
        env["cpu"] = subprocess.run(["powershell", "-NoProfile", "-Command", "(Get-CimInstance Win32_Processor).Name"],
                                    capture_output=True, text=True).stdout.strip()
    env.update(extra)
    return env


# ---- lab -----------------------------------------------------------------------------------------
def calibrate(name):
    a = time.perf_counter()
    with urllib.request.urlopen(f"http://127.0.0.1:{PORT}/gen/1/cal-{name}", timeout=5) as r:
        r.read()
    b = time.perf_counter()
    return round((a + b) / 2 * 1000, 3)


def lab(args):
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    clients = Clients(args, args.clients.split(","))
    lab_exe = find_labserver(args.labserver)
    if not lab_exe:
        sys.exit("the lab server isn't built: cargo build --release -p mdm-labserver")
    dest_dir = out / "dl"
    dest_dir.mkdir(exist_ok=True)
    log = out / "server.jsonl"
    server = subprocess.Popen([str(lab_exe), "--bind", f"127.0.0.1:{PORT}", "--log-file", str(log)],
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(1)
    clients.start()
    (out / "environment.json").write_text(json.dumps(environment(clients, {
        "server": f"labserver (this repo), 127.0.0.1:{PORT}, generated content",
        "settings": "DDM and curl as installed; aria2c split/connections as named, min-split-size 1M",
    }), indent=1))
    for c, note in clients.notes.items():
        print(f"skipped {c}: {note}", flush=True)
    scen = [s for s in scenarios_portable() if not args.only or s[0] in args.only.split(",")]
    runs = []
    try:
        for sid, title, size, faults, n, timeout in scen:
            n = args.runs or n
            print(f"== {sid}: {title} ({n} runs + warm-up)", flush=True)
            for i in range(-1, n):
                k = len(clients.wanted)
                order = clients.wanted[(i + 1) % k:] + clients.wanted[:(i + 1) % k]
                for c in order:
                    if (sid, c) in SKIP:
                        continue
                    tag = "w" if i < 0 else f"{i:02d}"
                    name = f"{sid}-{c}-{tag}.bin"
                    q = f"seed={i + 2}" + (f"&{faults}" if faults else "")
                    url = f"http://127.0.0.1:{PORT}/gen/{size}/{name}?{q}"
                    cal = calibrate(name)
                    r = run_once(clients, c, url, dest_dir / name, timeout, expected=size_bytes(size))
                    r.update(name=name, cal_ms=cal, scenario=sid, run=i, warmup=i < 0,
                             expected_bytes=size_bytes(size))
                    r.pop("timeline")
                    runs.append(r)
                    print(f"  {tag} {c}: {r['e2e_ms']} ms{'' if r['ok'] else '  FAILED ' + str(r['error'])}",
                          flush=True)
                    clean(dest_dir / name)
                    # Let the system write back what the run left in its cache before the next
                    # one starts (four clients writing 1 GiB each in a row saturated the disk:
                    # a run then waited 20 s for its own flush, whichever client it was).
                    time.sleep(0.5 + 6 * size_bytes(size) / 2**30)
                (out / "runs.json").write_text(json.dumps(runs, indent=1))
    finally:
        clients.stop()
        server.terminate()
    report(out)


# ---- internet ------------------------------------------------------------------------------------
def at(timeline, sec):
    """Bytes the client had at `sec` seconds (the last reading at or before it)."""
    got = 0
    for t, b in timeline:
        if t <= sec:
            got = b
    return got


def internet(args):
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    clients = Clients(args, args.clients.split(","))
    clients.start()
    dest_dir = out / "dl"
    dest_dir.mkdir(exist_ok=True)
    urls = args.url or INTERNET_URLS
    (out / "environment.json").write_text(json.dumps(environment(clients, {
        "settings": f"each client stopped after {args.seconds} s per address",
    }), indent=1))
    for c, note in clients.notes.items():
        print(f"skipped {c}: {note}", flush=True)
    results = []
    try:
        for rnd in range(args.rounds):
            for url in urls:
                k = len(clients.wanted)
                order = clients.wanted[rnd % k:] + clients.wanted[:rnd % k]
                for c in order:
                    dest = dest_dir / f"net-{c}.bin"
                    r = run_once(clients, c, url, dest, args.seconds + 30, stop_after=args.seconds)
                    tl = r["timeline"]
                    # Stopped at --seconds: the last reading is what it had then.
                    got = tl[-1][1] if tl else 0
                    first = next((t for t, b in tl if b > 0), None)
                    r.update(url=url, round=rnd, at_2s=at(tl, 2), at_5s=at(tl, 5), bytes=got,
                             first_byte_s=first,
                             seconds=round(r["e2e_ms"] / 1000 if r["ok"] else args.seconds, 2))
                    r["mb_s"] = round(got / 2**20 / max(r["seconds"], 0.001), 3)
                    results.append(r)
                    print(f"  {url.split('/')[2]:<36} {c:<9} {r['mb_s']:7.2f} MiB/s  "
                          f"({got / 2**20:.1f} MiB in {r['seconds']} s, 2 s: {r['at_2s'] / 2**20:.1f} MiB, "
                          f"connections {r['peak_connections']})"
                          f"{'' if r['ok'] or r['error'] == 'stopped' else '  ' + str(r['error'])}", flush=True)
                    clean(dest)
                    (out / "internet.json").write_text(json.dumps(results, indent=1))
                    time.sleep(1)
    finally:
        clients.stop()
    report(out)


# ---- report --------------------------------------------------------------------------------------
def report(out):
    out = Path(out)
    env = json.loads((out / "environment.json").read_text())
    lines = [f"# DDM vs other downloaders — {env['date']}", "",
             f"{env['cpu']}, {env['os']}. {env.get('server', 'real servers')}. {env.get('settings', '')}.", ""]
    lines += [f"- {k}: {v}" for k, v in env.get("clients", {}).items()]
    skipped = [f"- {k}: {v}" for k, v in env.get("skipped", {}).items()]
    if (out / "runs.json").exists():
        ran = {(r["scenario"], r["client"]) for r in json.loads((out / "runs.json").read_text())}
        scen = {s for s, _ in ran}
        skipped += [f"- {c} in {s}: {why}" for (s, c), why in SKIP.items() if s in scen and (s, c) not in ran]
    if skipped:
        lines += ["", "Skipped:", "", *skipped]
    if (out / "runs.json").exists():
        lines += ["", *lab_table(out)]
    if (out / "internet.json").exists():
        lines += ["", *internet_table(out)]
    (out / "REPORT.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("\n".join(lines))


def lab_table(out):
    import statistics as st
    from collections import defaultdict
    runs = json.loads((out / "runs.json").read_text())
    log = []
    if (out / "server.jsonl").exists():
        log = [json.loads(x) for x in (out / "server.jsonl").read_text().splitlines() if x.strip()]
    by_name = defaultdict(list)
    for e in log:
        by_name[e["path"].rsplit("/", 1)[-1]].append(e)
    # Without the server's log (a published run keeps only runs.json), the phases worked out
    # before are kept.
    for r in runs if log else []:
        reqs = [e for e in by_name.get(r["name"], []) if e.get("method") == "GET"]
        cal = by_name.get("cal-" + r["name"], [])
        r["requests"] = len(reqs)
        r["connections"] = len({e["peer"] for e in reqs})
        r["sent_ratio"] = round(sum(e["bytes_sent"] for e in reqs) / r["expected_bytes"], 3) if reqs else None
        r["transfer_ms"] = r["startup_ms"] = None
        good = [e for e in reqs if e["status"] in (200, 206)]
        if good and cal and r["ok"]:
            off = r["cal_ms"] - cal[0]["t_start_ms"]
            first = min(e["t_start_ms"] for e in reqs) + off
            last = max(e["t_end_ms"] for e in good) + off
            r["startup_ms"] = round(first - r["t0_ms"], 1)
            r["transfer_ms"] = round(last - first, 1)
    if log:
        (out / "runs.json").write_text(json.dumps(runs, indent=1))
    clients = list(dict.fromkeys(r["client"] for r in runs))
    scen = list(dict.fromkeys(r["scenario"] for r in runs))
    titles = {s[0]: s[1] for s in scenarios_portable()}
    q = lambda x, p: st.quantiles(x, n=4)[p] if len(x) > 1 else x[0]  # noqa: E731

    def cell(xs):
        if not xs:
            return "—"
        return f"{st.median(xs):.0f} ({q(xs, 0):.0f}–{q(xs, 2):.0f})"

    head = "| scenario | " + " | ".join(f"{c} median ms (IQR)" for c in clients) + " | " + \
           " | ".join(f"{c} / DDM" for c in clients if c != "ddm") + " |"
    lines = ["## Lab server: end to end (command -> complete file)", "", head,
             "|" + "---|" * (1 + len(clients) + len([c for c in clients if c != "ddm"]))]
    phases = ["", "Phases (median ms): startup / transfer; connections, requests, bytes sent / file size", ""]
    for s in scen:
        meas = {c: [r for r in runs if r["scenario"] == s and r["client"] == c and not r["warmup"]] for c in clients}
        ok = {c: [r["e2e_ms"] for r in rs if r["ok"]] for c, rs in meas.items()}
        failed = {c: len(rs) - len(ok[c]) for c, rs in meas.items()}
        base = st.median(ok["ddm"]) if ok.get("ddm") else None
        ratios = []
        for c in clients:
            if c == "ddm":
                continue
            ratios.append(f"{st.median(ok[c]) / base:.2f}×" if base and ok[c] else "—")
        cells = [cell(ok[c]) + (f" ({failed[c]} failed)" if failed[c] else "") for c in clients]
        lines.append(f"| {titles.get(s, s)} | " + " | ".join(cells) + " | " + " | ".join(ratios) + " |")
        for c in clients:
            rs = [r for r in meas[c] if r["ok"]]
            med = lambda k: (lambda v: f"{st.median(v):.0f}" if v else "—")(  # noqa: E731
                [r[k] for r in rs if r.get(k) is not None])
            medf = lambda k: (lambda v: f"{st.median(v):.2f}" if v else "—")(  # noqa: E731
                [r[k] for r in rs if r.get(k) is not None])
            phases.append(f"- {s} {c}: {med('startup_ms')} / {med('transfer_ms')}; connections {med('connections')}, "
                          f"requests {med('requests')}, sent {medf('sent_ratio')}, CPU {med('cpu_ms')} ms, "
                          f"peak memory {med('peak_ws_mb')} MB")
    return lines + phases


def internet_table(out):
    import statistics as st
    res = json.loads((out / "internet.json").read_text())
    clients = list(dict.fromkeys(r["client"] for r in res))
    urls = list(dict.fromkeys(r["url"] for r in res))
    lines = ["## Real servers: average speed over the first seconds (MiB/s, median of rounds)", "",
             "| address | " + " | ".join(clients) + " |", "|" + "---|" * (1 + len(clients))]
    for u in urls:
        cells = []
        for c in clients:
            rs = [r for r in res if r["url"] == u and r["client"] == c]
            good = [r["mb_s"] for r in rs if r["ok"] or r["error"] == "stopped"]
            conns = [r["peak_connections"] for r in rs if r.get("peak_connections")]
            cells.append((f"{st.median(good):.2f}" if good else "failed")
                         + (f" ({max(conns)} conn.)" if conns else ""))
        lines.append(f"| {u} | " + " | ".join(cells) + " |")
    return lines


def detect(args):
    clients = Clients(args, ["ddm", "curl", "aria2c", "motrix", "fdm"])
    print("found:", ", ".join(clients.wanted) or "nothing")
    for k, v in clients.versions().items():
        print(f"  {k}: {v}")
    for k, v in clients.notes.items():
        print(f"skipped {k}: {v}")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    stamp = dt.datetime.now().strftime("%Y%m%d-%H%M")
    for name in ("detect", "lab", "internet"):
        p = sub.add_parser(name)
        p.add_argument("--clients", default="ddm,curl,aria2c,motrix,fdm")
        p.add_argument("--out", default=str(ROOT / ".study" / "bench" / f"others-{name}-{stamp}"))
        p.add_argument("--ddm", help="DDM's program (default: CARGO_TARGET_DIR or target, release)")
        p.add_argument("--ddm-before", help="another build of DDM, run as client `ddm-before`")
        p.add_argument("--labserver")
        p.add_argument("--curl")
        p.add_argument("--aria2c")
        p.add_argument("--motrix-rpc", default="http://127.0.0.1:16800/jsonrpc")
        p.add_argument("--motrix-secret", default="")
        if name == "lab":
            p.add_argument("--runs", type=int, default=0, help="override runs per scenario")
            p.add_argument("--only", default="")
        if name == "internet":
            p.add_argument("--seconds", type=float, default=15)
            p.add_argument("--rounds", type=int, default=1)
            p.add_argument("--url", action="append")
    p = sub.add_parser("report")
    p.add_argument("dir")
    args = ap.parse_args()
    if args.cmd == "report":
        return report(args.dir)
    return {"detect": detect, "lab": lab, "internet": internet}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
