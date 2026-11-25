#!/usr/bin/env python3
"""F2-HMR: edita config/módulo sem restart, sem recriar WebView nem WebProcess.

Baseline esperado: dev-server.ts e enableHmr ausentes -> VEREDITO: FALHA.
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
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-hmr-", dir="/tmp/opencode"))
IPC = OUT / "ipc.sock"
INPUT = OUT / "input.sock"
LOG = OUT / "compositor.log"
HOST_LOG = OUT / "host.log"
RUNTIME_LOG = OUT / "runtime.log"
PIXELS = OUT / "pixels"
PIXELS.mkdir()
RUNTIME_DIR = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
SOCKET = f"wayland-aero-f2-hmr-{os.getpid()}"
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def wait_for(predicate, timeout=15):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if predicate():
                return True
        except (OSError, ValueError, KeyError, IndexError, json.JSONDecodeError):
            pass
        time.sleep(0.1)
    return False


def command(line, ipc=IPC):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.settimeout(5)
        sock.connect(str(ipc))
        sock.sendall((line + "\n").encode())
        lines = []
        for raw in sock.makefile("r", encoding="utf-8"):
            raw = raw.rstrip("\n")
            lines.append(raw)
            if raw == "." or raw.startswith(("OK", "ERR")):
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


def clean(path=LOG):
    paths = (LOG, HOST_LOG) if path == LOG else (path,)
    return ANSI.sub("", "\n".join(
        item.read_text(errors="replace") for item in paths if item.exists()
    ))


def process_tree():
    tree = {}
    for entry in pathlib.Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            status = (entry / "status").read_text()
            cmdline = (entry / "cmdline").read_bytes().replace(b"\0", b" ").decode(errors="replace")
        except OSError:
            continue
        ppid = None
        for line in status.splitlines():
            if line.startswith("PPid:"):
                ppid = int(line.split()[1])
                break
        tree[int(entry.name)] = (ppid, cmdline)
    return tree


def host_webprocesses(host_pid):
    tree = process_tree()
    result = []
    for pid, (ppid, cmdline) in tree.items():
        if "WebKitWebProcess" not in cmdline or "bwrap" in cmdline:
            continue
        seen = set()
        cursor = pid
        while cursor in tree and cursor not in seen:
            if cursor == host_pid:
                result.append(pid)
                break
            seen.add(cursor)
            cursor = tree[cursor][0]
            if cursor is None:
                break
    return sorted(result)


def pixel_counts():
    source = PIXELS / "full_000000.bgra"
    if not source.exists():
        return None
    data = source.read_bytes()
    red = green = blue = 0
    for i in range(0, len(data) - 3, 4):
        b, g, r, a = data[i:i + 4]
        if a < 200:
            continue
        if r > 150 and g < 90 and b < 90:
            red += 1
        if g > 130 and r < 90 and b < 90:
            green += 1
        if b > 150 and r < 90 and g < 90:
            blue += 1
    return red, green, blue


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def preflight():
    source = """
      import { startDevServer } from './packages/runtime/src/dev-server.ts';
      import { diffSurfaceDeclarations } from './packages/runtime/src/config-loader.ts';
      import { createClient } from './packages/sdk/src/client.ts';
      if (!startDevServer || !diffSurfaceDeclarations || !createClient) throw new Error('API HMR ausente');
    """
    result = subprocess.run(["bun", "-e", source], cwd=ROOT, capture_output=True, text=True)
    print("=== preflight HMR (cru) ===")
    print(result.stdout, end="")
    print(result.stderr, end="")
    return result.returncode == 0


def start_compositor(env, ipc, log_path, socket_name=SOCKET):
    handle = log_path.open("wb")
    proc = subprocess.Popen(
        [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
         "--socket", socket_name, "--size", "640x480"],
        cwd=ROOT, env={**env, "AERO_WINDOW_IPC_SOCK": str(ipc),
                       "AERO_TEST_INPUT_SOCK": str(OUT / f"{ipc.name}.input")}, 
        stdout=handle, stderr=subprocess.STDOUT, start_new_session=True,
    )
    if not wait_for(lambda: ipc.exists() and pathlib.Path(f"{ipc}.events").exists()):
        stop(proc)
        raise RuntimeError("compositor não ficou pronto")
    return proc


def main():
    if not preflight():
        print(f"LOGS: {OUT}")
        print("VEREDITO: FALHA")
        return 1
    port = free_port()
    origin = f"http://127.0.0.1:{port}"
    fixture = OUT / "fixture"
    fixture.mkdir()
    for name in ("desktop.config.tsx", "panel.ts", "background.ts"):
        shutil.copy(ROOT / "examples/blank-desktop" / name, fixture / name)
    config_path = fixture / "desktop.config.tsx"
    config_path.write_text(config_path.read_text().replace(
        '"../../packages/sdk/src/define"', json.dumps(str(ROOT / "packages/sdk/src/define.ts"))
    ))
    panel_fixture = fixture / "panel.ts"
    panel_fixture.write_text(panel_fixture.read_text().replace(
        '"../../packages/sdk/src/client"', json.dumps(str(ROOT / "packages/sdk/src/client.ts"))
    ))
    env = {
        **os.environ,
        "RUST_LOG": "info",
        "AERO_WINDOW_IPC_SOCK": str(IPC),
        "AERO_TEST_INPUT_SOCK": str(INPUT),
        "AERO_PIXEL_DUMP_DIR": str(PIXELS),
        "AERO_PIXEL_FULL": "1",
        "AERO_SURFACE_HOST": "1",
        "AERO_DEV_ORIGIN": origin,
    }
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
        env.pop(key, None)
    comp = host = runtime = production = None
    checks = {}
    try:
        production_ipc = OUT / "production.sock"
        production_env = {key: value for key, value in env.items() if key != "AERO_DEV_ORIGIN"}
        production = start_compositor(
            production_env, production_ipc, OUT / "production.log", f"{SOCKET}-prod"
        )
        spec = {
            "id": "http-fora", "kind": "panel", "anchors": {},
            "margins": [0, 0, 0, 0], "url": f"{origin}/surfaces/panel/",
        }
        response = command("surface-create " + json.dumps(spec), production_ipc)
        print(f"=== produção sem --dev: {response} ===")
        checks["sem dev recusa HTTP mesmo loopback"] = response.startswith("ERR")
        stop(production)
        production = None

        comp = start_compositor(env, IPC, LOG)
        handle = HOST_LOG.open("wb")
        host = subprocess.Popen(
            ["dbus-run-session", "--", str(ROOT / "target/debug/aeroshell-shell")],
            cwd=ROOT, env={**env, "WAYLAND_DISPLAY": SOCKET, "GDK_BACKEND": "wayland",
                           "GTK4_LAYER_SHELL_LIB": "libgtk4-layer-shell.so.0"},
            stdout=handle, stderr=subprocess.STDOUT, start_new_session=True,
        )
        checks["host conectou em dev"] = wait_for(lambda: "SURFACE_HOST pronto" in clean())
        runtime_handle = RUNTIME_LOG.open("wb")
        runtime = subprocess.Popen(
            ["bun", "packages/runtime/src/main.ts", "--config", str(config_path), "--dev"],
            cwd=ROOT, env=env, stdout=runtime_handle, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        original_pids = (comp.pid, runtime.pid)
        checks["dev server e duas superfícies iniciaram"] = wait_for(
            lambda: f"DEV_SERVER origin={origin}" in clean(RUNTIME_LOG)
            and "SURFACE_HOST criado" in clean()
            and clean().count("SURFACE_HOST criado") == 2
        )
        checks["bundle HTTP same-origin responde"] = wait_for(lambda: urllib.request.urlopen(
            f"{origin}/surfaces/blank-panel/", timeout=1
        ).status == 200)
        checks["WebProcess inicial identificado no host"] = wait_for(
            lambda: len(host_webprocesses(host.pid)) >= 1
        )
        checks["pixels iniciais do blank-desktop"] = wait_for(
            lambda: (pixel_counts() or (0, 0, 0))[0] > 150_000
            and (pixel_counts() or (0, 0, 0))[1] > 20_000
        )
        time.sleep(0.5)

        def measured_webprocesses(stage):
            pids = host_webprocesses(host.pid)
            print(f"=== WebProcesses {stage}: {pids} ===")
            return len(pids)

        initial_webprocesses = measured_webprocesses("iniciais no host dev")

        original = config_path.read_text()
        changed = original.replace("#e00000", "#0000e0").replace(
            "blank-desktop · SDK sem framework", "HMR_CONFIG_OK"
        )
        config_path.write_text(changed)
        checks["edição do TSX muda DOM e pixels sem restart"] = wait_for(
            lambda: (pixel_counts() or (0, 0, 0))[2] > 150_000
            and "HMR_CONFIG_OK" in clean()
        )
        checks["PIDs do compositor/runtime permanecem"] = (comp.pid, runtime.pid) == original_pids
        checks["update não recria WebViews/layers"] = (
            clean().count("SURFACE_HOST criado id=blank-background") == 1
            and clean().count("SURFACE_HOST criado id=blank-panel") == 1
            and "SURFACE_HOST load_uri id=blank-panel" in clean()
        )
        checks["WebProcess não cresce no update"] = measured_webprocesses("após update") <= initial_webprocesses

        diff_count = clean(RUNTIME_LOG).count("HMR_DIFF")
        panel_path = fixture / "panel.ts"
        panel_path.write_text(panel_path.read_text().replace("MODULO_ANTES", "MODULO_DEPOIS"))
        checks["WebSocket recarrega módulo sem diff IPC"] = wait_for(
            lambda: "MODULO_DEPOIS" in clean()
        ) and clean(RUNTIME_LOG).count("HMR_DIFF") == diff_count
        checks["WebProcess não cresce no reload WS"] = measured_webprocesses("após reload WS") <= initial_webprocesses

        current = config_path.read_text()
        overlay = '''    { kind: "Overlay", id: "hmr-extra", anchors: { left: true, right: true, top: true, bottom: true }, html: "<!doctype html><html><body style='background:#800080'>extra</body></html>" },\n'''
        created = current.replace("  ],\n  frame:", overlay + "  ],\n  frame:")
        config_path.write_text(created)
        checks["diff cria superfície nova"] = wait_for(
            lambda: "SURFACE_HOST criado id=hmr-extra" in clean()
            and "create=hmr-extra" in clean(RUNTIME_LOG)
        )
        config_path.write_text(current)
        checks["diff destrói superfície removida"] = wait_for(
            lambda: "SURFACE_HOST destruido" in clean()
            and "destroy=hmr-extra" in clean(RUNTIME_LOG)
        )
        checks["create/destroy preserva WebProcess"] = measured_webprocesses("após create/destroy") <= initial_webprocesses

        external = {**spec, "id": "externa", "url": "https://example.com/"}
        checks["origem externa é recusada em dev"] = command(
            "surface-create " + json.dumps(external)
        ).startswith("ERR")
        other = {**spec, "id": "outra-loopback", "url": "http://127.0.0.1:1/painel"}
        checks["outra origem loopback é recusada"] = command(
            "surface-create " + json.dumps(other)
        ).startswith("ERR")
        command("surface-update " + json.dumps({
            "id": "blank-panel", "url": f"{origin}/surfaces/blank-panel/?external-test=1",
        }))
        checks["navegação externa posterior é bloqueada"] = wait_for(
            lambda: "SURFACE_HOST navegação externa bloqueada" in clean()
        )
        checks["zero erros de protocolo"] = "ProtocolError" not in clean()
    except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired) as err:
        checks["execução sem exceção"] = False
        print(f"EXCEÇÃO: {err}")
    finally:
        print(f"LOGS: {OUT}")
        print("=== runtime (cru) ===")
        print(clean(RUNTIME_LOG)[-5000:])
        print("=== host (recorte cru) ===")
        for line in clean().splitlines():
            if any(token in line for token in ("SURFACE_HOST", "guest", "HMR")):
                print(line)
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {label}")
        verdict = "OK" if checks and all(checks.values()) else "FALHA"
        print("VEREDITO: " + verdict)
        for proc in (runtime, host, comp, production):
            stop(proc)
    return 0 if verdict == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
