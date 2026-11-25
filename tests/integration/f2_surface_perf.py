#!/usr/bin/env python3
"""F2-surface-perf: medicao A/B de responsividade e WebProcesses (sem corrucao).

Pre-requisito: cargo build -p aeroshell-compositor --features bench
Uso: python3 tests/integration/f2_surface_perf.py
"""
import json
import os
import pathlib
import re
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from datetime import datetime

ROOT = pathlib.Path(__file__).resolve().parents[2]
COMP = ROOT / "target/debug/aeroshell-compositor"
FRAME = ROOT / "target/debug/examples/frame_webview"
HOST = ROOT / "target/debug/aeroshell-shell"
RUNTIME = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
ANSI = re.compile(r"\x1b\[[0-9;]*m")
HZ = 100


def log_read(path):
    return ANSI.sub("", path.read_text(errors="replace")) if path.exists() else ""


def wait_for(predicate, timeout=20):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if predicate():
                return True
        except (OSError, ValueError, KeyError, IndexError, json.JSONDecodeError):
            pass
        time.sleep(0.1)
    return False


def ipc_command(path, line):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.settimeout(5)
        sock.connect(str(path))
        sock.sendall((line + "\n").encode())
        stream = sock.makefile("r", encoding="utf-8")
        lines = []
        while True:
            current = stream.readline()
            if not current:
                break
            current = current.rstrip("\n")
            lines.append(current)
            if current == "." or current.startswith(("OK", "ERR")):
                break
        return lines


def cpu_times(pid):
    try:
        fields = pathlib.Path(f"/proc/{pid}/stat").read_text().split()
        return int(fields[13]) + int(fields[14])
    except (OSError, IndexError, ValueError):
        return None


def process_tree():
    tree = {}
    for entry in pathlib.Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            status = (entry / "status").read_text()
        except OSError:
            continue
        pid = int(entry.name)
        ppid = None
        for line in status.splitlines():
            if line.startswith("PPid:"):
                ppid = int(line.split()[1])
                break
        cmdline = ""
        try:
            cmdline = (entry / "cmdline").read_bytes().replace(b"\0", b" ").decode(errors="replace")
        except OSError:
            pass
        tree[pid] = (ppid, cmdline)
    return tree


def webprocess_owners(roots):
    tree = process_tree()
    owners = {name: [] for name in roots}
    for pid, (ppid, cmdline) in tree.items():
        if "WebKitWebProcess" not in cmdline:
            continue
        seen = set()
        cursor = pid
        owner = "outro"
        while cursor in tree and cursor not in seen:
            seen.add(cursor)
            parent, _ = tree[cursor]
            for name, root_pid in roots.items():
                if cursor == root_pid:
                    owner = name
                    break
            if owner != "outro":
                break
            if parent is None:
                break
            cursor = parent
        owners[owner].append(pid)
    return owners


def event_reader(path):
    values = []

    class Reader:
        def __init__(self):
            self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self.sock.connect(str(path))
            self.thread = threading.Thread(target=self._read, daemon=True)
            self.thread.start()

        def _read(self):
            try:
                for line in self.sock.makefile("r", encoding="utf-8"):
                    try:
                        values.append(json.loads(line))
                    except json.JSONDecodeError:
                        pass
            except OSError:
                pass

        def close(self):
            try:
                self.sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            self.sock.close()
            self.thread.join(timeout=2)

    return Reader(), values


def damage_area(value):
    if value in ("none", "submitted", "", None):
        return 0
    total = 0
    for rect in re.split(r"[;,]", value or ""):
        match = re.fullmatch(r"(\d+)x(\d+)@-?\d+,-?\d+", rect)
        if match:
            total += int(match.group(1)) * int(match.group(2))
    return total


def bench_counts(text):
    frames = re.findall(r"OUTPUT_FRAME t=(\d+) output=\S+ seq=\d+ age=\d+ damage=(\S+)", text)
    commits = re.findall(r"FRAME_BUFFER_COMMIT buffer=Some\((\d+), (\d+)\)", text)
    return {
        "output_frames": len(frames),
        "damage_pixels_total": sum(damage_area(value) for _, value in frames),
        "commits": len(commits),
        "sizes": sorted({f"{w}x{h}" for w, h in commits}),
    }


def headless_damage(text):
    rows = re.findall(r"HEADLESS_FRAME output=\S+ age=(\d+) damage=(none|[0-9x@,;-]+)", text)
    return {
        "frames": len(rows),
        "age_gt_zero": sum(1 for age, _ in rows if int(age) > 0),
        "damage_none": sum(1 for _, damage in rows if damage == "none"),
        "damage_pixels_total": sum(damage_area(damage) for _, damage in rows),
        "amostras": rows[-3:],
    }


def percentile(samples, q):
    if not samples:
        return None
    ordered = sorted(samples)
    return ordered[min(len(ordered) - 1, int(len(ordered) * q))]


def stop_group(proc):
    if proc is None or proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait(timeout=5)


def start_scenario(with_surfaces, label, backend="winit"):
    out = pathlib.Path(tempfile.mkdtemp(prefix=f"aero-f2-perf-{label}-", dir="/tmp/opencode"))
    ipc = out / "ipc.sock"
    inp = out / "input.sock"
    log = out / "compositor.log"
    env = {
        **os.environ,
        "RUST_LOG": "info",
        "AERO_WINDOW_IPC_SOCK": str(ipc),
        "AERO_TEST_INPUT_SOCK": str(inp),
    }
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
        env.pop(key, None)
    sock = f"wayland-aero-perf-{label}-{os.getpid()}"
    handle = log.open("wb")
    comp_args = [str(COMP), "--backend", backend, "--socket", sock]
    if backend == "headless":
        comp_args += ["--size", "640x480"]
    comp_args += ["--shell", str(FRAME), "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"]
    comp = subprocess.Popen(
        comp_args,
        cwd=ROOT, env=env, stdout=handle, stderr=subprocess.STDOUT, start_new_session=True,
    )
    if not wait_for(lambda: ipc.exists() and pathlib.Path(f"{ipc}.events").exists(), 25):
        raise RuntimeError(f"{label}: IPC nao ficou pronto")
    reader, event_values = event_reader(pathlib.Path(f"{ipc}.events"))
    feet = []
    for title in ("A", "B"):
        feet.append(subprocess.Popen(
            ["foot", "-T", f"foot {title}"], cwd=ROOT,
            env={**env, "WAYLAND_DISPLAY": sock, "GDK_BACKEND": "wayland"},
            stdout=handle, stderr=subprocess.STDOUT, start_new_session=True,
        ))
    wait_for(lambda: len([l for l in ipc_command(ipc, "list") if l.startswith("W ")]) >= 2, 25)
    host = None
    if with_surfaces:
        host = subprocess.Popen(
            ["dbus-run-session", "--", str(HOST)], cwd=ROOT,
            env={**env, "WAYLAND_DISPLAY": sock, "GDK_BACKEND": "wayland",
                 "GTK4_LAYER_SHELL_LIB": "libgtk4-layer-shell.so.0", "AERO_SURFACE_HOST": "1"},
            stdout=handle, stderr=subprocess.STDOUT, start_new_session=True,
        )
        wait_for(lambda: "SURFACE_HOST pronto" in log_read(log), 20)
        base = {"output": 0, "anchors": {"left": True, "right": True, "top": True, "bottom": True},
                "margins": [0, 0, 0, 0], "exclusive_zone": 0}
        specs = [
            dict(base, id="desk", kind="desktop", layer="background", keyboard_mode="none", input_region=None,
                 html="<html><body style='background:#e00000'><div style='position:absolute;inset:0;background:#e00000'></div></body></html>"),
            dict(base, id="over", kind="overlay", layer="overlay", keyboard_mode="ondemand",
                 input_region=[{"x": 0, "y": 0, "width": 2000, "height": 60}],
                 html="<html><body style='background:transparent'><div style='position:absolute;inset:0;border:60px solid #00d000'></div></body></html>"),
        ]
        for spec in specs:
            result = ipc_command(ipc, "surface-create " + json.dumps(spec, separators=(",", ":")))
            print(f"[{label}] surface-create {spec['id']}: {result}")
        wait_for(lambda: "SURFACE_HOST criado id=over" in log_read(log), 20)
    return {
        "label": label, "out": out, "ipc": ipc, "inp": inp, "log": log, "sock": sock,
        "comp": comp, "host": host, "feet": feet, "reader": reader, "events": event_values,
    }


def cpu_delta(scenario, seconds):
    pids = {"compositor": scenario["comp"].pid,
            "frame_shell": None, "host": scenario["host"].pid if scenario["host"] else None}
    tree = process_tree()
    for pid, (_, cmdline) in tree.items():
        if "frame_webview" in cmdline:
            pids["frame_shell"] = pid
    before = {name: cpu_times(pid) for name, pid in pids.items() if pid}
    t0 = time.monotonic()
    time.sleep(seconds)
    elapsed = time.monotonic() - t0
    after = {name: cpu_times(pid) for name, pid in pids.items() if pid}
    return {name: round(100 * (after[name] - before[name]) / HZ / elapsed, 1)
            for name in before if after.get(name) is not None}


def inject(scenario, line):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.connect(str(scenario["inp"]))
        sock.sendall((line + "\n").encode())


def measure_latency(scenario, phase):
    results = {"click_ms": [], "text_ms": []}
    for i in range(8):
        line = scenario["log"]
        before_commits = len(re.findall(r"FOOT_COMMIT t=", log_read(line)))
        t0 = time.monotonic()
        inject(scenario, "text a")
        deadline = time.monotonic() + 1.5
        while time.monotonic() < deadline:
            if len(re.findall(r"FOOT_COMMIT t=", log_read(line))) > before_commits:
                results["text_ms"].append(round((time.monotonic() - t0) * 1000, 1))
                break
            time.sleep(0.005)
        windows = [l for l in ipc_command(scenario["ipc"], "list") if l.startswith("W ")]
        if windows:
            fields = windows[i % len(windows)].split()
            x, y = int(fields[2]) + int(fields[4]) // 2, int(fields[3]) + int(fields[5]) // 2
            before_focus = len(re.findall(r"BUTTON_FOCUS .*pressed=true", log_read(scenario["log"])))
            t0 = time.monotonic()
            inject(scenario, f"motion {x} {y}")
            inject(scenario, "button press")
            inject(scenario, "button release")
            deadline = time.monotonic() + 1.5
            while time.monotonic() < deadline:
                if len(re.findall(r"BUTTON_FOCUS .*pressed=true", log_read(scenario["log"]))) > before_focus:
                    results["click_ms"].append(round((time.monotonic() - t0) * 1000, 1))
                    break
                time.sleep(0.005)
    print(f"[{scenario['label']}/{phase}] latencia ms: " + json.dumps({
        k: {"n": len(v), "p50": percentile(v, 0.5), "p99": percentile(v, 0.99), "amostras": v}
        for k, v in results.items()
    }))
    return results


def stages(scenario):
    text = log_read(scenario["log"])
    patterns = [
        ("compositor_up", r"iniciando AeroShell"),
        ("ipc_up", r"IPC de janelas ativo"),
        ("cliente_connect", r"cliente Wayland conectado ao socket público"),
        ("superficie_host", r"SURFACE_HOST pronto"),
        ("superficie_criada", r"SURFACE_HOST criado id=over"),
        ("frame_requested", r"FRAMEWEB: frame_requested window_id"),
        ("frame_painted", r"FRAMEWEB: frame_painted window_id"),
    ]
    stamp = re.compile(r"^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d+Z)")
    out = {}
    base = None
    for name, pattern in patterns:
        match = re.search(pattern, text)
        if not match:
            out[name] = None
            continue
        line_start = text.rfind("\n", 0, match.start()) + 1
        time_match = stamp.match(text[line_start:])
        if time_match:
            when = datetime.fromisoformat(time_match.group(1).replace("Z", "+00:00"))
            if base is None:
                base = when
            out[name] = round((when - base).total_seconds() * 1000, 1)
        else:
            out[name] = match.group(0)[:80]
    return out


def run():
    if not COMP.exists() or not HOST.exists():
        print("FALHA: build os binarios antes (bench por ultimo)")
        return 1
    report = {}
    for with_surfaces, label, backend in (
        (False, "A-foot", "winit"),
        (True, "B-surfaces", "winit"),
        (True, "C-headless", "headless"),
    ):
        scen = None
        try:
            scen = start_scenario(with_surfaces, label, backend)
            time.sleep(3)
            roots = {"compositor": scen["comp"].pid,
                     "host": scen["host"].pid if scen["host"] else -1,
                     "frame": None}
            tree = process_tree()
            for pid, (_, cmdline) in tree.items():
                if "frame_webview" in cmdline and "aeroshell" not in cmdline:
                    roots["frame"] = pid
            before = webprocess_owners(roots)
            cpu = cpu_delta(scen, 5)
            after = webprocess_owners(roots)
            bench_before = bench_counts(log_read(scen["log"]))
            time.sleep(5)
            bench_after = bench_counts(log_read(scen["log"]))
            early = measure_latency(scen, "0-10s")
            time.sleep(25)
            late = measure_latency(scen, "35-45s")
            report[label] = {
                "webprocess_antes": before,
                "webprocess_depois": after,
                "total_webprocess": sum(len(v) for v in after.values()),
                "cpu_pct_5s": cpu,
                "bench_delta_5s": {
                    k: bench_after[k] - bench_before[k]
                    for k in ("output_frames", "damage_pixels_total", "commits")
                },
                "tamanhos_commit": bench_after["sizes"],
                "latencia_inicial": early,
                "latencia_tardia": late,
                "headless_damage": headless_damage(log_read(scen["log"])) if backend == "headless" else None,
                "estagios": stages(scen),
                "log": str(scen["log"]),
            }
        except Exception as err:
            report[label] = {"erro": repr(err)}
        finally:
            if scen:
                for proc in scen["feet"]:
                    stop_group(proc)
                if scen["host"]:
                    stop_group(scen["host"])
                stop_group(scen["comp"])
                scen["reader"].close()
    print("=== RELATORIO F2-SURFACE-PERF (cru) ===")
    for label, data in report.items():
        print(f"--- {label} ---")
        print(json.dumps(data, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(run())
