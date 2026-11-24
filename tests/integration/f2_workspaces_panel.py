#!/usr/bin/env python3
"""F2-workspaces: pré-filtro do painel React com workspaces reais.

Sobe o nested com 2 `foot` e o `aeroshell-shell`, lê o `panel-state` publicado
pelo bundle e injeta cliques reais no botão `+` e na aba de workspace. O juiz
visual final continua sendo o humano.

Pré-requisitos:
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-shell
  bun build packages/panel/src/index.tsx --target=browser --outfile=packages/panel/dist/app.js

Uso: python3 tests/integration/f2_workspaces_panel.py
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
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNTIME_DIR = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-workspaces-panel-", dir="/tmp/opencode"))
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def wait_for(predicate, timeout=15):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


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


def clean(path):
    return ANSI.sub("", path.read_text(errors="replace")) if path.exists() else ""


def panel_state(log):
    for raw in reversed(re.findall(r"mensagem da WebView json=(.+)$", clean(log), re.M)):
        try:
            value = json.loads(raw)
        except json.JSONDecodeError:
            continue
        if isinstance(value, str):
            try:
                value = json.loads(value)
            except json.JSONDecodeError:
                continue
        if isinstance(value, dict) and value.get("type") == "panel-state":
            return value
    return None


def panel_geometry(log):
    found = re.findall(
        r"geometry\s*=\s*Some\(Rectangle<[^>]*>\s*\{\s*x:\s*(-?\d+),\s*y:\s*(-?\d+)",
        clean(log),
    )
    return tuple(map(int, found[-1])) if found else (0, 0)


def click(input_path, geometry, rect):
    x, y = geometry
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.connect(str(input_path))
        for line in (
            f"motion {x + int(rect['x']) + int(rect['w']) // 2} "
            f"{y + int(rect['y']) + int(rect['h']) // 2}",
            "button press",
            "button release",
        ):
            sock.sendall((line + "\n").encode())
            time.sleep(0.25)


def main():
    sock = f"wayland-aero-f2-ws-panel-{os.getpid()}"
    ipc = OUT / "ipc.sock"
    input_path = OUT / "input.sock"
    log = OUT / "compositor.log"
    env = {**os.environ, "RUST_LOG": "debug", "AERO_WINDOW_IPC_SOCK": str(ipc),
           "AERO_TEST_INPUT_SOCK": str(input_path)}
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
        env.pop(key, None)
    comp = shell = None
    feet = []
    checks = {}
    try:
        handle = log.open("wb")
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "winit",
             "--socket", sock, "--size", "1000x600",
             "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, env=env, stdout=handle, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        wait_for(lambda: (RUNTIME_DIR / sock).exists() and ipc.exists(), 15)
        shell_env = {
            **env, "WAYLAND_DISPLAY": sock, "GDK_BACKEND": "wayland",
            "GTK4_LAYER_SHELL_LIB": os.environ.get("GTK4_LAYER_SHELL_LIB", "libgtk4-layer-shell.so.0"),
        }
        shell = subprocess.Popen(
            ["dbus-run-session", "--", str(ROOT / "target/debug/aeroshell-shell")],
            cwd=ROOT, env=shell_env, stdout=handle, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        for title in ("painel-workspace-A", "painel-workspace-B"):
            feet.append(subprocess.Popen(
                ["foot", "-T", title], cwd=ROOT,
                env={**env, "WAYLAND_DISPLAY": sock, "GDK_BACKEND": "wayland"},
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                start_new_session=True,
            ))

        checks["painel lista workspaces"] = wait_for(
            lambda: (panel_state(log) or {}).get("workspaces", [{}])[0].get("id") == 1
        )
        checks["painel lista 2 foot"] = wait_for(
            lambda: len((panel_state(log) or {}).get("rows", [])) == 2
        )
        state = panel_state(log) or {}
        checks["aba 1 ativa no estado"] = state.get("activeWorkspace") == 1

        click(input_path, panel_geometry(log), state["addRect"])
        checks["botão + cria workspace no painel"] = wait_for(
            lambda: len((panel_state(log) or {}).get("workspaces", [])) == 2
        )
        state = panel_state(log) or {}
        second = next((ws["id"] for ws in state["workspaces"] if ws["id"] != 1), None)
        checks["nova aba publicou geometria interativa"] = str(second) in (state.get("workspaceRects") or {})

        click(input_path, panel_geometry(log), state["workspaceRects"][str(second)])
        checks["clique na aba ativa workspace 2"] = wait_for(
            lambda: (panel_state(log) or {}).get("activeWorkspace") == second
        )
        checks["ativação esconde as 2 janelas do workspace 1"] = wait_for(
            lambda: (panel_state(log) or {}).get("rows", []) == []
        )

        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock_ipc:
            sock_ipc.connect(str(ipc))
            sock_ipc.sendall(b"window-workspace 0 2\n")
            assert sock_ipc.recv(1024).startswith(b"OK")
        checks["mover janela atualiza a lista do painel"] = wait_for(
            lambda: [row["id"] for row in (panel_state(log) or {}).get("rows", [])] == [0]
        )
        state = panel_state(log) or {}
        click(input_path, panel_geometry(log), state["workspaceRects"]["1"])
        checks["voltar à aba 1 mostra só a outra janela"] = wait_for(
            lambda: [row["id"] for row in (panel_state(log) or {}).get("rows", [])] == [1]
        )
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock_ipc:
            sock_ipc.connect(str(ipc))
            sock_ipc.sendall(b'workspace-rename 2 "Pesquisa"\n')
            assert sock_ipc.recv(1024).startswith(b"OK")
        checks["nome editável por IPC aparece no painel"] = wait_for(
            lambda: any(ws.get("name") == "Pesquisa" for ws in (panel_state(log) or {}).get("workspaces", []))
        )
        checks["zero erros de protocolo"] = "ProtocolError" not in clean(log)
    except (OSError, AssertionError, KeyError, TypeError) as err:
        checks["cenário do painel sem exceção"] = False
        print(f"EXCEÇÃO: {err}")
    finally:
        print(f"LOGS: {OUT}")
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {label}")
        verdict = "OK" if checks and all(checks.values()) else "FALHA"
        print("VEREDITO: " + verdict)
        for proc in feet:
            stop(proc)
        stop(shell)
        stop(comp)
    return 0 if verdict == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
