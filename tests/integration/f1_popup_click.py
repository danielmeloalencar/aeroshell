#!/usr/bin/env python3
"""Diagnóstico do juiz visual negado (F1-subsurfaces): clique em xdg_popup com grab.

Pre-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-compositor --example popup_click_client
  cargo build -p aeroshell-shell --example frame_webview

Roda no headless, sem display; fecha somente os PIDs que iniciou.
Uso: python3 tests/integration/f1_popup_click.py
"""
import os
import pathlib
import re
import socket
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-popup-click-"))
SOCK = f"wayland-aero-popup-click-{os.getpid()}"
RUNTIME = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
# Geometria REAL lida da sonda: POPUP_COMMIT ... toplevel_coords=(0,0) ->
# popup em (60,80)-(180,120). O pai vai ate (260,230).
POPUP_POINTS = [(120, 100), (100, 95), (150, 110), (80, 105)]
PARENT_POINT = (220, 200)


def send_input(path, line):
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.connect(path)
            s.sendall(f"{line}\n".encode())
    except OSError:
        pass


def main() -> int:
    comp_log = OUT / "compositor.log"
    client_err = OUT / "client.err"
    input_sock = str(OUT / "input.sock")
    backend = os.environ.get("AERO_TEST_BACKEND", "headless")
    comp = subprocess.Popen(
        [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", backend,
         "--socket", SOCK, "--shell", str(ROOT / "target/debug/examples/frame_webview"),
         "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
        cwd=ROOT, stdout=comp_log.open("wb"), stderr=subprocess.STDOUT,
        env={**os.environ, "RUST_LOG": "debug", "AERO_TEST_INPUT_SOCK": input_sock},
    )
    client = None
    try:
        deadline = time.time() + 15
        while time.time() < deadline and not (RUNTIME / SOCK).exists():
            time.sleep(0.05)
        if not (RUNTIME / SOCK).exists():
            print("FALHA: socket não apareceu")
            return 1
        time.sleep(0.2)
        client = subprocess.Popen(
            [str(ROOT / "target/debug/examples/popup_click_client")], cwd=ROOT,
            stdout=subprocess.DEVNULL, stderr=client_err.open("wb"),
            env={**os.environ, "WAYLAND_DISPLAY": SOCK},
        )
        deadline = time.time() + 15
        while time.time() < deadline and client.poll() is None:
            if "POPUP_CLICK popup_configured" in client_err.read_text(errors="replace"):
                break
            time.sleep(0.05)
        for i in range(30):
            if i % 5 == 4:
                x, y = PARENT_POINT
            else:
                x, y = POPUP_POINTS[i % len(POPUP_POINTS)]
            send_input(input_sock, f"motion {x} {y}")
            send_input(input_sock, "button press")
            send_input(input_sock, "button release")
            time.sleep(0.1)
        deadline = time.time() + 15
        while time.time() < deadline and client.poll() is None:
            time.sleep(0.1)
        if client.poll() is None:
            client.terminate()
            client.wait(timeout=5)

        out = client_err.read_text(errors="replace")
        log = comp_log.read_text(errors="replace")
        print("=== cliente ===")
        print(out, end="")
        checks = {
            "popup configurado": "POPUP_CLICK popup_configured" in out,
            "grab aceito no compositor": bool(re.search(r"POPUP_GRAB t=\d+ .* estado=ok", log)),
            # grab_popup() só CRIA o PopupGrab: tem que ser instalado no ponteiro.
            # Sem isso `is_grabbed()` é false no clique do item (causa do Qt).
            "grab instalado no ponteiro": bool(re.search(r"BUTTON_FOCUS t=\d+ .* pressed=true .* grabbed=true", log)),
            "enter no popup": "POPUP_CLICK enter label=popup" in out,
            "botão entregue ao popup": "POPUP_CLICK button label=popup" in out,
            "botão entregue ao pai": "POPUP_CLICK button label=parent" in out,
            "cliente terminou OK": "POPUP_CLICK OK" in out,
            "sem erro de protocolo": not re.search(r"error \d+:|protocol error", log, re.I),
        }
        for name, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {name}")
        print(f"log_cru={OUT}")
        print("VEREDITO: " + ("OK" if all(checks.values()) else "FALHA"))
        return 0 if all(checks.values()) else 1
    finally:
        if client is not None and client.poll() is None:
            client.terminate()
            client.wait(timeout=5)
        if comp.poll() is None:
            comp.terminate()
            comp.wait(timeout=5)


if __name__ == "__main__":
    sys.exit(main())
