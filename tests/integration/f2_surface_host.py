#!/usr/bin/env python3
"""F2-surface-host: desktop + overlay por IPC, z-order, pixels e clique transparente.

Baseline esperado antes do cartão: `surface-create` não existe → VEREDITO: FALHA.

Pré-requisitos (bench POR ÚLTIMO):
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-shell
  cargo build --workspace --examples

Uso: python3 tests/integration/f2_surface_host.py
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

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-surface-host-", dir="/tmp/opencode"))
IPC = OUT / "ipc.sock"
INPUT = OUT / "input.sock"
LOG = OUT / "compositor.log"
PIXELS = OUT / "pixels"
PIXELS.mkdir()
RUNTIME = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
SOCKET = f"wayland-aero-f2-surface-{os.getpid()}"
OUTPUT = (640, 480)
ANSI = re.compile(r"\x1b\[[0-9;]*m")

DESKTOP_HTML = (
    "<!doctype html><html><head><style>html,body{width:100%;height:100%;margin:0;"
    "background:#e00000}</style></head><body>"
    "<div style='position:absolute;inset:0;background:#e00000'></div></body></html>"
)
OVERLAY_HTML = (
    "<!doctype html><html><head><style>html,body{width:100%;height:100%;margin:0;"
    "background:transparent}.borda{position:absolute;inset:0;border:60px solid #00d000;"
    "box-sizing:border-box;pointer-events:none}.中心{position:absolute;left:180px;top:140px;"
    "width:280px;height:200px;background:transparent}</style></head>"
    "<body><div class=borda></div></body></html>"
)


def wait_for(predicate, timeout=12):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if predicate():
                return True
        except (KeyError, IndexError, TypeError, OSError, json.JSONDecodeError):
            pass
        time.sleep(0.05)
    return False


def command(line):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.settimeout(5)
        sock.connect(str(IPC))
        sock.sendall((line + "\n").encode())
        stream = sock.makefile("r", encoding="utf-8")
        first = stream.readline().rstrip("\n")
        return first


def stop(proc):
    if proc is None or proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        proc.wait(timeout=6)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait(timeout=5)


def clean():
    return ANSI.sub("", LOG.read_text(errors="replace")) if LOG.exists() else ""


class Events:
    def __init__(self, path):
        self.values = []
        self.lock = threading.Lock()
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(str(path))
        self.thread = threading.Thread(target=self._read, daemon=True)
        self.thread.start()

    def _read(self):
        try:
            for line in self.sock.makefile("r", encoding="utf-8"):
                try:
                    value = json.loads(line)
                except json.JSONDecodeError:
                    continue
                with self.lock:
                    self.values.append(value)
        except OSError:
            pass

    def kinds(self):
        with self.lock:
            return [next(iter(value), "") for value in self.values]

    def has(self, kind, surface_id=None):
        with self.lock:
            for value in self.values:
                if kind not in value:
                    continue
                payload = value[kind]
                if surface_id is None or payload.get("id") == surface_id or payload.get("surface", {}).get("id") == surface_id:
                    return True
        return False

    def close(self):
        try:
            self.sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.sock.close()
        self.thread.join(timeout=2)


def surface_spec(id, kind, html, **overrides):
    spec = {
        "id": id,
        "kind": kind,
        "output": 0,
        "anchors": {"left": True, "right": True, "top": True, "bottom": True},
        "margins": [0, 0, 0, 0],
        "exclusive_zone": 0,
        "layer": "background" if kind == "desktop" else "overlay",
        "keyboard_mode": "none" if kind == "desktop" else "ondemand",
        "input_region": None,
        "html": html,
    }
    spec.update(overrides)
    return spec


def create_surface(spec):
    return command("surface-create " + json.dumps(spec, separators=(",", ":")))


def pixel_counts():
    path = PIXELS / "full_000000.bgra"
    if not path.exists():
        return None
    data = path.read_bytes()
    (PIXELS / "full_snapshot.bgra").write_bytes(data)
    red = green = 0
    for i in range(0, len(data) - 3, 4):
        b, g, r, a = data[i:i + 4]
        if a < 200:
            continue
        if r > 150 and g < 90 and b < 90:
            red += 1
        if g > 130 and r < 90 and b < 90:
            green += 1
    return red, green


def surface_id(pattern):
    found = re.findall(pattern, clean())
    return found[-1] if found else None


def click(x, y):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.connect(str(INPUT))
        for line in (f"motion {x} {y}", "button press", "button release"):
            sock.sendall((line + "\n").encode())
            time.sleep(0.2)


def main():
    env = {
        **os.environ,
        "RUST_LOG": "debug",
        "AERO_WINDOW_IPC_SOCK": str(IPC),
        "AERO_TEST_INPUT_SOCK": str(INPUT),
        "AERO_PIXEL_DUMP_DIR": str(PIXELS),
        "AERO_PIXEL_FULL": "1",
        "AERO_SURFACE_HOST": "1",
    }
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
        env.pop(key, None)
    comp = host = foot = None
    events = None
    checks = {}
    try:
        handle = LOG.open("wb")
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
             "--socket", SOCKET, "--size", f"{OUTPUT[0]}x{OUTPUT[1]}"],
            cwd=ROOT, env=env, stdout=handle, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        checks["compositor headless e IPC prontos"] = wait_for(
            lambda: (RUNTIME / SOCKET).exists() and IPC.exists() and pathlib.Path(f"{IPC}.events").exists(),
            15,
        )
        if not checks["compositor headless e IPC prontos"]:
            raise RuntimeError("compositor/IPC não ficou pronto")
        events = Events(pathlib.Path(f"{IPC}.events"))

        host = subprocess.Popen(
            ["dbus-run-session", "--", str(ROOT / "target/debug/aeroshell-shell")],
            cwd=ROOT,
            env={**env, "WAYLAND_DISPLAY": SOCKET, "GDK_BACKEND": "wayland",
                 "GTK4_LAYER_SHELL_LIB": "libgtk4-layer-shell.so.0"},
            stdout=handle, stderr=subprocess.STDOUT, start_new_session=True,
        )
        checks["host de superfícies conectou"] = wait_for(
            lambda: "SURFACE_HOST pronto" in clean(), 12
        )

        desktop = {"id": "desk", "kind": "desktop", "html": DESKTOP_HTML}
        # A região de input é a BORDA OPACA; o centro transparente não captura
        # clique e deve repassá-lo ao cliente (prévia do hit-test por alfa).
        overlay = {
            "id": "over", "kind": "overlay", "html": OVERLAY_HTML,
            "input_region": [
                {"x": 0, "y": 0, "width": OUTPUT[0], "height": 60},
                {"x": 0, "y": 420, "width": OUTPUT[0], "height": 60},
                {"x": 0, "y": 60, "width": 60, "height": 360},
                {"x": 580, "y": 60, "width": 60, "height": 360},
            ],
        }
        # 1) desktop sozinho: medir o fundo sem nada por cima.
        raw_desktop = create_surface(surface_spec(**desktop))
        print(f"=== surface-create desktop (cru): {raw_desktop} ===")
        checks["surface-create desktop responde OK"] = raw_desktop == "OK"
        checks["SurfaceCreated do desktop no stream"] = wait_for(
            lambda: events.has("SurfaceCreated", "desk")
        )
        checks["host cria o desktop"] = wait_for(
            lambda: "SURFACE_HOST criado id=desk kind=desktop" in clean()
        )

        early = None
        deadline = time.time() + 8
        while time.time() < deadline:
            early = pixel_counts()
            if early and early[0] > 200:
                break
            time.sleep(0.2)
        (PIXELS / "full_before_foot.bgra").write_bytes(
            (PIXELS / "full_snapshot.bgra").read_bytes()
        )
        print(f"=== pixels desktop isolado: {early} ===")
        checks["desktop vermelho aparece nos pixels"] = bool(early and early[0] > 200)

        # 2) overlay com borda opaca e região de input só na borda.
        raw_overlay = create_surface(surface_spec(**overlay))
        print(f"=== surface-create overlay (cru): {raw_overlay} ===")
        checks["surface-create overlay responde OK"] = raw_overlay == "OK"
        checks["SurfaceCreated do overlay no stream"] = wait_for(
            lambda: events.has("SurfaceCreated", "over")
        )
        checks["host cria o overlay"] = wait_for(
            lambda: "SURFACE_HOST criado id=over kind=overlay" in clean()
        )

        foot_env = {**env, "WAYLAND_DISPLAY": SOCKET}
        foot = subprocess.Popen(
            ["foot", "-w", "300x220"], cwd=ROOT, env=foot_env,
            stdout=handle, stderr=subprocess.STDOUT, start_new_session=True,
        )
        checks["cliente foot mapeado"] = wait_for(lambda: any(
            len(line.split()) >= 6 and line.split()[4].isdigit() and int(line.split()[4]) > 0
            for line in command("list").splitlines() if line.startswith("W ")
        ))
        print("=== IPC list cru ===")
        print(command("list"))

        deadline = time.time() + 10
        counts = None
        while time.time() < deadline:
            counts = pixel_counts()
            if counts and counts[0] > 200 and counts[1] > 200:
                break
            time.sleep(0.2)
        print(f"=== pixels vermelho/verde: {counts} ===")
        checks["borda verde do overlay aparece nos pixels"] = bool(counts and counts[1] > 200)
        checks["z-order desktop < cliente < overlay medido"] = wait_for(
            lambda: re.search(r"SURFACE_Z .*desktop.*cliente.*overlay", clean()) is not None
        )

        target = (OUTPUT[0] // 2, OUTPUT[1] // 2)
        wait_for(lambda: surface_id(r"CLIENT_ELEMENT_PROBE [^\n]*surface=ObjectId\([^,]+, (\d+)\)") is not None)
        foot_surface = surface_id(r"CLIENT_ELEMENT_PROBE [^\n]*surface=ObjectId\([^,]+, (\d+)\)")
        print(f"=== alvo do clique: {target}, superfície do foot: {foot_surface} ===")
        click(*target)
        checks["clique na região transparente chega ao cliente"] = bool(foot_surface) and wait_for(
            lambda: surface_id(r"BUTTON_FOCUS .*focus=Some\(ObjectId\([^,]+, (\d+)\)") == foot_surface
        )
        print(f"=== foco após clique: {surface_id(r'BUTTON_FOCUS .*focus=Some\(ObjectId\([^,]+, (\d+)\)')} ===")

        raw_update = command("surface-update " + json.dumps({
            "id": "over", "kind": "overlay", "html": OVERLAY_HTML,
            "margins": [10, 10, 10, 10], "exclusive_zone": 4,
        }, separators=(",", ":")))
        print(f"=== surface-update (cru): {raw_update} ===")
        checks["surface-update responde OK e publica evento"] = raw_update == "OK" and wait_for(
            lambda: events.has("SurfaceUpdated", "over")
        )

        raw_destroy = command("surface-destroy over")
        print(f"=== surface-destroy (cru): {raw_destroy} ===")
        checks["surface-destroy remove e publica evento"] = raw_destroy == "OK" and wait_for(
            lambda: events.has("SurfaceDestroyed", "over")
            and "SURFACE_HOST destruido id=over" in clean()
        )
        checks["zero erros de protocolo"] = "ProtocolError" not in clean()
    except (OSError, RuntimeError, IndexError, AttributeError, json.JSONDecodeError, subprocess.TimeoutExpired) as err:
        checks["execução do cenário sem exceção"] = False
        print(f"EXCEÇÃO: {err}")
    finally:
        print(f"LOGS: {OUT}")
        print("=== eventos (cru) ===")
        if events is not None:
            print(" ".join(events.kinds()))
            events.close()
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {label}")
        verdict = "OK" if checks and all(checks.values()) else "FALHA"
        print("VEREDITO: " + verdict)
        for proc in (foot, host, comp):
            stop(proc)
    return 0 if verdict == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
