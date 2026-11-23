#!/usr/bin/env python3
"""F1-csd-gtk: o global org_kde_kwin_server_decoration_manager existe.

GTK3/gedit pede decoração por este protocolo (libgdk-3.so.0), não pelo
zxdg_toplevel_decoration. Baseline: global AUSENTE. Depois: presente.

Pre-requisito: cargo build -p aeroshell-compositor --example kde_globals_client
Uso: python3 tests/integration/f1_kde_decoration.py
"""
import os
import pathlib
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path("/tmp/opencode/f1-kde")
OUT.mkdir(parents=True, exist_ok=True)
SOCK = f"wayland-aerokde-{os.getpid()}"
LOG = OUT / f"comp-{os.getpid()}.log"

env = os.environ.copy()
for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET"):
    env.pop(key, None)
comp = subprocess.Popen(
    [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless", "--socket", SOCK],
    cwd=ROOT, env=env, stdout=LOG.open("wb"), stderr=subprocess.STDOUT,
)
try:
    deadline = time.time() + 12
    while time.time() < deadline and not pathlib.Path("/run/user/1000", SOCK).exists():
        if comp.poll() is not None:
            break
        time.sleep(0.1)
    time.sleep(0.4)
    client = subprocess.run(
        [str(ROOT / "target/debug/examples/kde_globals_client")],
        cwd=ROOT, env={**os.environ, "WAYLAND_DISPLAY": SOCK},
        capture_output=True, text=True,
    )
    out = client.stderr
    present = "KDE_CLIENT CHECK org_kde_kwin_server_decoration_manager presente" in out
    print(out[-600:])
    print(f"{'OK ' if present else 'FALHA '} global org_kde_kwin_server_decoration_manager")
    print("VEREDITO: " + ("OK" if present else "FALHA"))
    sys.exit(0 if present else 1)
finally:
    if comp.poll() is None:
        comp.terminate()
        comp.wait(timeout=5)
