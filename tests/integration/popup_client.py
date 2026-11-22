#!/usr/bin/env python3
"""F1-popups-render: cliente xdg_popup real + grab explícito no compositor.

Pré-requisito: `cargo build -p aeroshell-compositor --features bench` e
`cargo build -p aeroshell-compositor --example popup_client`. Roda no headless,
sem display; fecha somente os PIDs que iniciou.

Uso: python3 tests/integration/popup_client.py
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
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-popup-"))
SOCK = f"wayland-aero-popup-{os.getpid()}"


def wait_for_socket(path: pathlib.Path, log: pathlib.Path, timeout=10.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if path.exists() or (log.exists() and b"Created new socket" in log.read_bytes()):
            return True
        time.sleep(0.05)
    return False


def main() -> int:
    comp_log = OUT / "compositor.log"
    client_err = OUT / "client.err"
    comp = subprocess.Popen(
        [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
         "--socket", SOCK, "--shell", str(ROOT / "target/debug/examples/frame_webview"),
         "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
        cwd=ROOT, stdout=comp_log.open("wb"), stderr=subprocess.STDOUT,
        env={**os.environ, "RUST_LOG": "debug"},
    )
    client = None
    try:
        if not wait_for_socket(pathlib.Path("/run/user/1000") / SOCK, comp_log):
            print("FALHA: compositor não criou o socket")
            return 1
        env = {**os.environ, "WAYLAND_DISPLAY": SOCK}
        time.sleep(0.3)
        client = subprocess.Popen(
            [str(ROOT / "target/debug/examples/popup_client")],
            cwd=ROOT, stdout=subprocess.DEVNULL, stderr=client_err.open("wb"), env=env,
        )
        deadline = time.time() + 20
        while time.time() < deadline and client.poll() is None:
            time.sleep(0.1)
        if client.poll() is None:
            client.terminate()
            client.wait(timeout=5)

        out = client_err.read_text(errors="replace")
        log = comp_log.read_text(errors="replace")
        checks = {
            "cliente mapeou popup": "POPUP_CLIENT popup_configured" in out,
            "cliente pediu grab": "POPUP_CLIENT grab_requested" in out,
            "cliente dispensou sem erro": "POPUP_CLIENT dismissed" in out,
            "cliente terminou OK": "POPUP_CLIENT OK" in out,
            "POPUP_NEW no compositor": "POPUP_NEW t=" in log,
            "POPUP_GRAB aceito": bool(re.search(r"POPUP_GRAB t=\d+ serial=\d+ estado=ok", log)),
            "POPUP_COMMIT": "POPUP_COMMIT t=" in log,
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
