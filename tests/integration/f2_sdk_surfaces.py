#!/usr/bin/env python3
"""F2-sdk-surfaces: desktop.config.tsx cria background+panel somente pelo SDK.

Baseline antes da implementação: imports de aero.surfaces/createClient/defineDesktop
falham → VEREDITO: FALHA. Depois mede pixels, z-order, SurfaceUpdated do guest e
clique que atravessa as regiões transparentes até o foot.

Pré-requisitos (bench POR ÚLTIMO):
  bun build packages/sdk/src/client.ts --target=browser --outfile=packages/sdk/dist/client.js
  cargo build --workspace --examples
  cargo build -p aeroshell-shell
  cargo build -p aeroshell-compositor --features bench
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
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-sdk-surfaces-", dir="/tmp/opencode"))
IPC = OUT / "ipc.sock"
INPUT = OUT / "input.sock"
LOG = OUT / "compositor.log"
RUNTIME_LOG = OUT / "runtime.log"
PIXELS = OUT / "pixels"
PIXELS.mkdir()
RUNTIME_DIR = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
SOCKET = f"wayland-aero-f2-sdk-{os.getpid()}"
OUTPUT = (640, 480)
ANSI = re.compile(r"\x1b\[[0-9;]*m")


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


def command(line, dot=False):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.settimeout(5)
        sock.connect(str(IPC))
        sock.sendall((line + "\n").encode())
        lines = []
        for raw in sock.makefile("r", encoding="utf-8"):
            raw = raw.rstrip("\n")
            if raw == "." and dot:
                break
            lines.append(raw)
            if not dot:
                break
        return "\n".join(lines)


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


def runtime_clean():
    return RUNTIME_LOG.read_text(errors="replace") if RUNTIME_LOG.exists() else ""


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

    def has(self, kind, surface_id=None, predicate=None):
        with self.lock:
            for value in self.values:
                if kind not in value:
                    continue
                payload = value[kind]
                if surface_id is not None:
                    actual = payload.get("id") or payload.get("surface", {}).get("id")
                    if actual != surface_id:
                        continue
                if predicate is None or predicate(payload):
                    return True
        return False

    def kinds(self):
        with self.lock:
            return [next(iter(value), "") for value in self.values]

    def close(self):
        try:
            self.sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.sock.close()
        self.thread.join(timeout=2)


def pixel_snapshot():
    source = PIXELS / "full_000000.bgra"
    if not source.exists():
        return None
    data = source.read_bytes()
    target = PIXELS / "full_snapshot.bgra"
    target.write_bytes(data)
    return data


def pixel_counts():
    data = pixel_snapshot()
    if data is None:
        return None
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


def preflight():
    source = """
      import { createAero } from './packages/sdk/src/index.ts';
      import { createClient } from './packages/sdk/src/client.ts';
      import { defineDesktop } from './packages/sdk/src/define.ts';
      const aero = createAero(async () => ['OK']);
      if (!aero.surfaces || !createClient || !defineDesktop) throw new Error('API ausente');
    """
    result = subprocess.run(["bun", "-e", source], cwd=ROOT, capture_output=True, text=True)
    print("=== preflight da API (cru) ===")
    print(result.stdout, end="")
    print(result.stderr, end="")
    return result.returncode == 0


def main():
    if not preflight():
        print("VEREDITO: FALHA")
        return 1
    env = {
        **os.environ,
        "RUST_LOG": "debug",
        "AERO_WINDOW_IPC_SOCK": str(IPC),
        "AERO_TEST_INPUT_SOCK": str(INPUT),
        "AERO_PIXEL_DUMP_DIR": str(PIXELS),
        "AERO_PIXEL_FULL": "1",
        "AERO_SURFACE_HOST": "1",
    }
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD", "AERO_DESKTOP_CONFIG"):
        env.pop(key, None)
    comp = host = runtime = foot = None
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
            lambda: (RUNTIME_DIR / SOCKET).exists() and IPC.exists()
            and pathlib.Path(f"{IPC}.events").exists(), 15,
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
        checks["host de superfícies conectou"] = wait_for(lambda: "SURFACE_HOST pronto" in clean(), 12)
        runtime_handle = RUNTIME_LOG.open("wb")
        runtime = subprocess.Popen(
            ["bun", "packages/runtime/src/main.ts", "--config",
             "examples/blank-desktop/desktop.config.tsx"],
            cwd=ROOT, env=env, stdout=runtime_handle, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        checks["config TS cria background e panel pelo SDK"] = wait_for(
            lambda: "CONFIG_SURFACES count=2" in runtime_clean()
            and "SURFACE_HOST criado id=blank-background" in clean()
            and "SURFACE_HOST criado id=blank-panel" in clean(), 15,
        )
        checks["eventos SurfaceCreated consumidos no runtime"] = wait_for(
            lambda: events.has("SurfaceCreated", "blank-background")
            and events.has("SurfaceCreated", "blank-panel")
        )

        counts = None
        deadline = time.time() + 10
        while time.time() < deadline:
            counts = pixel_counts()
            if counts and counts[0] > 150_000 and counts[1] > 20_000:
                break
            time.sleep(0.2)
        print(f"=== pixels background/panel: {counts} ===")
        checks["background vermelho e painel verde medidos"] = bool(
            counts and counts[0] > 150_000 and counts[1] > 20_000
        )
        checks["guest atualiza a PRÓPRIA região input/blur sem reload"] = wait_for(
            lambda: events.has(
                "SurfaceUpdated", "blank-panel",
                lambda payload: payload.get("surface", {}).get("blur_region") is not None,
            ) and clean().count("SURFACE_HOST html id=blank-panel") == 1, 10,
        )

        foot = subprocess.Popen(
            ["foot", "-w", "300x220"], cwd=ROOT,
            env={**env, "WAYLAND_DISPLAY": SOCKET}, stdout=handle,
            stderr=subprocess.STDOUT, start_new_session=True,
        )
        checks["cliente foot mapeado"] = wait_for(lambda: any(
            len(line.split()) >= 6 and line.split()[4].isdigit() and int(line.split()[4]) > 0
            for line in command("list", dot=True).splitlines() if line.startswith("W ")
        ))
        print("=== IPC list cru ===")
        print(command("list", dot=True))
        checks["z-order desktop < cliente < panel"] = wait_for(
            lambda: "SURFACE_Z desktop < cliente < overlay" in clean()
        )
        target = (OUTPUT[0] // 2, OUTPUT[1] // 2)
        wait_for(lambda: surface_id(
            r"CLIENT_ELEMENT_PROBE [^\n]*surface=ObjectId\([^,]+, (\d+)\)"
        ) is not None)
        foot_surface = surface_id(r"CLIENT_ELEMENT_PROBE [^\n]*surface=ObjectId\([^,]+, (\d+)\)")
        click(*target)
        checks["clique atravessa área transparente até o foot"] = bool(foot_surface) and wait_for(
            lambda: surface_id(r"BUTTON_FOCUS .*focus=Some\(ObjectId\([^,]+, (\d+)\)") == foot_surface
        )
        checks["surface-list retorna as duas specs tipadas"] = wait_for(
            lambda: "CONFIG_LIST ids=blank-background,blank-panel" in runtime_clean()
        )
        checks["zero erros de protocolo"] = "ProtocolError" not in clean()
    except (OSError, RuntimeError, IndexError, AttributeError, json.JSONDecodeError, subprocess.TimeoutExpired) as err:
        checks["execução do cenário sem exceção"] = False
        print(f"EXCEÇÃO: {err}")
    finally:
        print(f"LOGS: {OUT}")
        print("=== runtime (cru) ===")
        print(runtime_clean()[-3000:])
        print("=== eventos (cru) ===")
        if events is not None:
            print(" ".join(events.kinds()))
            events.close()
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {label}")
        verdict = "OK" if checks and all(checks.values()) else "FALHA"
        print("VEREDITO: " + verdict)
        for proc in (foot, runtime, host, comp):
            stop(proc)
    return 0 if verdict == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
