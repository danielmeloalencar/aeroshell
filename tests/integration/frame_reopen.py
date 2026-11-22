#!/usr/bin/env python3
"""Correção F1: abrir -> fechar -> reabrir foot deve manter a decoração.

Baseline (bug): o compositor reutiliza window_id = count() e o shell não remove
FrameEntry no Closed, então o novo frame_requested cai em "já apresentado;
ignorado" e a janela nova abre sem decoração.

Pré-requisito: cargo build -p aeroshell-compositor --features bench &&
cargo build --workspace --examples
"""
import json
import os
import pathlib
import re
import socket
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
BIN = ROOT / "target/debug/aeroshell-compositor"
SHELL = ROOT / "target/debug/examples/frame_webview"
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-reopen-"))
SOCK = f"wayland-aero-reopen-{os.getpid()}"
IPC = OUT / "ipc.sock"
LOG = OUT / "compositor.log"


def ipc(cmd):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(5)
    s.connect(str(IPC))
    s.sendall(f"{cmd}\n".encode())
    data = b""
    while not data.endswith(b"\n"):
        chunk = s.recv(4096)
        if not chunk:
            break
        data += chunk
    s.close()
    return data.decode(errors="replace").strip()


def list_ids():
    resp = ipc("list")
    ids = []
    for line in resp.splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[0] == "W":
            ids.append(int(parts[1]))
    return ids


def wait_for(predicate, timeout=20.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        text = LOG.read_text(errors="replace") if LOG.exists() else ""
        if predicate(text):
            return text
        time.sleep(0.1)
    return LOG.read_text(errors="replace") if LOG.exists() else ""


def main():
    env = {**os.environ, "RUST_LOG": "info", "AERO_WINDOW_IPC_SOCK": str(IPC)}
    comp = subprocess.Popen(
        [str(BIN), "--backend", "winit", "--socket", SOCK,
         "--shell", str(SHELL), "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
        cwd=ROOT, stdout=LOG.open("wb"), stderr=subprocess.STDOUT, env=env,
    )
    feet = []
    try:
        deadline = time.time() + 15
        while time.time() < deadline:
            if (pathlib.Path("/run/user/1000") / SOCK).exists() and IPC.exists():
                break
            time.sleep(0.1)
        if not IPC.exists():
            print("FALHA: IPC não apareceu")
            return 1

        foot_env = {**os.environ, "WAYLAND_DISPLAY": SOCK}
        first = subprocess.Popen(["foot"], cwd=ROOT, stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL, env=foot_env)
        feet.append(first)
        wait_for(lambda t: "associate" in t and "window_id=0" in t)
        ids1 = list_ids()
        print(f"ids após 1º foot: {ids1}")
        if not ids1:
            print("FALHA: nenhuma janela listada")
            return 1
        old = ids1[0]

        print(f"close {old}: {ipc(f'close {old}')}")
        deadline = time.time() + 10
        while time.time() < deadline and list_ids():
            time.sleep(0.2)
        print(f"ids após close: {list_ids()}")

        second = subprocess.Popen(["foot"], cwd=ROOT, stdout=subprocess.DEVNULL,
                                   stderr=subprocess.DEVNULL, env=foot_env)
        feet.append(second)
        text = wait_for(lambda t: t.count("toplevel mapeado") >= 2, timeout=20)
        text = wait_for(lambda t: "associate" in t or "já apresentado; ignorado" in t, timeout=10)
        # Exigir PINTURA real (não só placeholder) na decoração da nova janela.
        time.sleep(3)
        text = wait_for(
            lambda t: "FRAME_PAINTED" in t or "SLICE_COMPOSE" in t,
            timeout=10,
        )
        ids2 = list_ids()
        text = LOG.read_text(errors="replace")
        print(f"ids após reabrir: {ids2}")

        new_ids = [i for i in ids2 if i != old]
        reused = bool(new_ids) and all(i == old for i in ids2)
        ignored = "já apresentado; ignorado" in text
        painted_novo = False
        if new_ids:
            painted_novo = any(
                f"window_id={new_ids[0]}" in line
                and (
                    "FRAME_PAINTED" in line
                    or "frame_painted" in line
                    or "SLICE_COMPOSE" in line
                )
                for line in text.splitlines()
            )
        checks = {
            "nova janela tem id diferente": bool(new_ids) and not reused,
            "nenhum 'já apresentado; ignorado'": not ignored,
            "decoração da nova janela": painted_novo,
        }
        for name, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {name}")
        print(f"log_cru={OUT}")
        print("VEREDITO: " + ("OK" if all(checks.values()) else "FALHA"))
        return 0 if all(checks.values()) else 1
    finally:
        for p in feet:
            if p.poll() is None:
                p.terminate()
                p.wait(timeout=5)
        if comp.poll() is None:
            comp.terminate()
            comp.wait(timeout=5)


if __name__ == "__main__":
    sys.exit(main())
