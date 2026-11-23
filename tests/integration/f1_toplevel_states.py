#!/usr/bin/env python3
"""F1-xdg-toplevel-states: fullscreen, maximize, parent e min/max por cliente real.

Pre-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-compositor --example toplevel_states_client
  cargo build -p aeroshell-shell --example frame_webview

Uso: python3 tests/integration/f1_toplevel_states.py
"""
import os
import pathlib
import socket
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNTIME = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-xdg-states-"))
SOCK = f"wayland-aero-xdgstates-{os.getpid()}"
COMP_LOG = OUT / "comp.log"
CLIENT_LOG = OUT / "client.log"


def log_text():
    return COMP_LOG.read_text(errors="replace") if COMP_LOG.exists() else ""


def client_text():
    return CLIENT_LOG.read_text(errors="replace") if CLIENT_LOG.exists() else ""


def wait_for(predicate, timeout=10):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


def send_command(proc, command):
    proc.stdin.write(f"{command}\n".encode())
    proc.stdin.flush()


def ipc_list(path):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(5)
        s.connect(path)
        s.sendall(b"list\n")
        buf = b""
        while b".\n" not in buf:
            chunk = s.recv(4096)
            if not chunk:
                break
            buf += chunk
    return buf.decode(errors="replace")


def main():
    input_sock = str(OUT / "input.sock")
    ipc_sock = str(OUT / "ipc.sock")
    env = {**os.environ, "RUST_LOG": "debug", "AERO_TEST_INPUT_SOCK": input_sock,
           "AERO_WINDOW_IPC_SOCK": ipc_sock}
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET"):
        env.pop(key, None)
    comp = subprocess.Popen(
        [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
         "--socket", SOCK, "--size", "800x600",
         "--shell", str(ROOT / "target/debug/examples/frame_webview"),
         "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
        cwd=ROOT, stdout=COMP_LOG.open("wb"), stderr=subprocess.STDOUT, env=env,
    )
    client = None
    try:
        deadline = time.time() + 15
        while time.time() < deadline and not (RUNTIME / SOCK).exists():
            if comp.poll() is not None:
                print("FALHA: compositor saiu antes do socket")
                return 1
            time.sleep(0.05)
        time.sleep(0.3)
        client = subprocess.Popen(
            [str(ROOT / "target/debug/examples/toplevel_states_client")],
            cwd=ROOT, stdout=subprocess.DEVNULL, stderr=CLIENT_LOG.open("wb"),
            stdin=subprocess.PIPE,
            env={**os.environ, "WAYLAND_DISPLAY": SOCK},
        )
        if not wait_for(lambda: "XDG_CLIENT WINDOW_CREATED win=0" in client_text()):
            print("FALHA: cliente não criou a janela")
            return 1
        # O frame WebView associa depois do bootstrap do pool; sem esperar, o
        # configure de fullscreen chega antes do associate e o DOM não é tocado.
        if not wait_for(lambda: "FRAMEWEB: associate(window_id=0)" in log_text(), timeout=20):
            print("AVISO: frame WebView não associou a tempo (DOM não avaliado)")

        print("=== cliente (cru) ===")
        print(client_text(), end="")
        comp_text = log_text()

        # 1. min/max no configure inicial (cliente pediu 400x300..900x700).
        import re
        initial = re.search(r"INITIAL_CONFIGURE .* w=(\d+) h=(\d+)", comp_text)
        initial_ok = bool(initial and int(initial.group(1)) <= 900 and int(initial.group(2)) <= 700
                          and int(initial.group(1)) >= 400 and int(initial.group(2)) >= 300)

        # 2. fullscreen → output cheio, sem frame; depois restaura.
        send_command(client, "fullscreen")
        fs_ok = wait_for(lambda: "XDG_STATE" in log_text() and "fullscreen=true" in log_text()
                         and "FRAME_STATE" in log_text() and "fullscreen=true" in log_text())
        # Geometria pelo IPC: o log do compositor pode intercalar com o do shell.
        fs_ipc = ipc_list(ipc_sock)
        fs_size_ok = any(
            line.startswith("W 0 0 0 800 600 ") and "mode=fullscreen" in line
            for line in fs_ipc.splitlines()
        )
        fs_dom_ok = wait_for(lambda: "csd_applied" in log_text()
                             and "bar=hidden border=hidden" in log_text(), timeout=8)
        fs_client_ok = wait_for(lambda: bool(re.search(
            r"XDG_CLIENT CONFIGURE win=0.*states=.*fullscreen", client_text())))
        client_mark = len(client_text())
        send_command(client, "unfullscreen")
        # O log do compositor pode intercalar com o do shell; o configure do
        # cliente é a fonte confiável da transição de estado.
        restore_ok = wait_for(lambda: any(
            "fullscreen" not in line
            for line in client_text()[client_mark:].splitlines()
            if "XDG_CLIENT CONFIGURE win=0" in line
        )) and wait_for(lambda: "frame_restored" in log_text(), timeout=8)

        # 3. maximize pela work area + clamp de min/max + restore.
        send_command(client, "maximize")
        max_ok = wait_for(lambda: bool(re.search(
            r"XDG_STATE[^\n]*maximized=true", log_text())))
        m = re.search(r"XDG_STATE[^\n]*maximized=true[^\n]*w=(\d+) h=(\d+)", log_text())
        max_clamped = bool(m and int(m.group(1)) <= 900 and int(m.group(2)) <= 700)
        client_mark = len(client_text())
        send_command(client, "unmaximize")
        unmax_ok = wait_for(lambda: any(
            "maximized" not in line
            for line in client_text()[client_mark:].splitlines()
            if "XDG_CLIENT CONFIGURE win=0" in line
        ))

        # 4. parent/transient: diálogo acima do pai, focado e marcado no IPC.
        send_command(client, "parent")
        parent_ok = wait_for(lambda: "XDG_PARENT" in log_text())
        ipc = ipc_list(ipc_sock)
        transient_ok = "transient=1" in ipc
        send_command(client, "destroy_parent")
        child_gone = wait_for(lambda: "XDG_CLIENT CHILD_DESTROYED" in client_text())

        send_command(client, "quit")
        deadline = time.time() + 5
        while time.time() < deadline and client.poll() is None:
            time.sleep(0.05)
        if client.poll() is None:
            client.terminate()
            client.wait(timeout=5)

        comp_text = log_text()
        checks = {
            "min/max no configure inicial": initial_ok,
            "fullstate fullscreen=true no compositor": fs_ok and fs_size_ok and fs_client_ok,
            "frame escondido no fullscreen (DOM)": fs_dom_ok,
            "unfullscreen restaura": restore_ok,
            "maximize por protocolo": max_ok,
            "maximize respeita min/max": max_clamped,
            "unmaximize restaura": unmax_ok,
            "set_parent registrado": parent_ok,
            "transiente marcado no IPC": transient_ok,
            "filho destruído sem erro": child_gone and "ProtocolError" not in comp_text,
        }
        print("=== compositor (linhas de estado) ===")
        for line in comp_text.splitlines():
            if any(tag in line for tag in ("XDG_STATE", "FRAME_STATE", "XDG_PARENT", "INITIAL_CONFIGURE",
                                           "csd_applied", "frame_restored", "ProtocolError")):
                print(line)
        print("=== IPC list ===")
        print(ipc)
        for name, passed in checks.items():
            print(f"{'OK ' if passed else 'FALHA '} {name}")
        verdict = "OK" if all(checks.values()) else "FALHA"
        print("VEREDITO: " + verdict)
        return 0 if verdict == "OK" else 1
    finally:
        if client is not None and client.poll() is None:
            client.terminate()
            try:
                client.wait(timeout=5)
            except subprocess.TimeoutExpired:
                client.kill()
                client.wait(timeout=5)
        if comp.poll() is None:
            comp.terminate()
            comp.wait(timeout=5)


if __name__ == "__main__":
    sys.exit(main())
