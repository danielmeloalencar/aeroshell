#!/usr/bin/env python3
"""F1-toplevel-icon: ícone real do protocolo até o DOM da titlebar.

Pre-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-compositor --example toplevel_icon_client
  cargo build -p aeroshell-shell --example frame_webview

Roda no nested (precisa de DISPLAY), fecha somente os PIDs que iniciou.
Uso: python3 tests/integration/f1_toplevel_icon.py
"""
import base64
import os
import pathlib
import re
import signal
import socket
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path("/tmp/opencode/f1-icon")
OUT.mkdir(parents=True, exist_ok=True)
SOCK = f"wayland-aero-icon-{os.getpid()}"
LOG = OUT / f"comp-{os.getpid()}.log"
FOOT_ERR = OUT / f"foot-{os.getpid()}.err"
IPC = OUT / f"ipc-{os.getpid()}.sock"

for orphan in (f"/run/user/1000/{SOCK}", str(IPC)):
    if os.path.exists(orphan):
        os.unlink(orphan)

env = os.environ.copy()
for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(RUST_LOG="debug", AERO_WINDOW_IPC_SOCK=str(IPC))

comp = subprocess.Popen(
    [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "winit",
     "--socket", SOCK, "--size", "900x600",
     "--command", str(ROOT / "target/debug/examples/toplevel_icon_client"),
     "--shell", str(ROOT / "target/debug/examples/frame_webview"),
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
    cwd=ROOT, env=env, stdout=LOG.open("wb"), stderr=subprocess.STDOUT,
)
env_foot = {**os.environ, "WAYLAND_DISPLAY": SOCK, "GDK_BACKEND": "wayland"}
env_foot.pop("DISPLAY", None)
foot = subprocess.Popen(
    ["foot"], cwd=ROOT, stdout=subprocess.DEVNULL, stderr=FOOT_ERR.open("wb"),
    env=env_foot, start_new_session=True,
)


def cleanup():
    if foot is not None and foot.poll() is None:
        try:
            os.killpg(foot.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()
            comp.wait(timeout=5)


def clean(path: pathlib.Path) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", path.read_text(errors="replace")) if path.exists() else ""


def ipc(cmd: str) -> str:
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(3)
            s.connect(str(IPC))
            s.sendall(f"{cmd}\n".encode())
            s.shutdown(socket.SHUT_WR)
            return s.recv(65536).decode(errors="replace")
    except OSError as err:
        return f"ERR {err}"


try:
    deadline = time.time() + 40
    # O cliente é lançado pelo compositor; o stderr dele vai para o LOG dele.
    while time.time() < deadline and "ICON_CLIENT OK" not in clean(LOG):
        if comp.poll() is not None:
            break
        time.sleep(0.1)
    time.sleep(2)  # deixa o shell aplicar no polling de 1 s
    log = clean(LOG)
    client_out = log

    icon_line = ""
    raw_pixels = b""
    for line in ipc("icon 0").splitlines():
        if line.startswith("ICON "):
            icon_line = line
            parts = line.split()
            if len(parts) >= 7:
                try:
                    raw_pixels = base64.b64decode(parts[6])
                except Exception:
                    raw_pixels = b""
            break

    time.sleep(6)
    log = clean(LOG)
    foot_err = clean(FOOT_ERR)

    red_bgra = len(raw_pixels) >= 4 and raw_pixels[:4] == b"\x00\x00\xff\xff"
    checks = {
        "global xdg_toplevel_icon_manager_v1": "ICON_CLIENT global xdg_toplevel_icon_manager_v1" in client_out,
        "CHECK do cliente": "ICON_CLIENT CHECK xdg_toplevel_icon_manager_v1 presente" in client_out,
        "cliente anexou ícone": "ICON_CLIENT icon_set size=16x16" in client_out,
        "sonda ICON_SET (bench)": bool(re.search(r"ICON_SET t=\d+ window_id=0 w=16 h=16 scale=1", log)),
        "IPC icon 0 responde": icon_line.startswith("ICON 0 16 16"),
        "pixels BGRA vermelhos": red_bgra,
        "DOM recebeu data URI": "icon_applied window_id=0" in log
        and "src=data:image/png" in log,
        "sonda titlebar_icon": bool(re.search(
            r"FRAMEWEB: titlebar_icon window_id=0 w=16 h=16 source=protocolo", log)),
        "foot resolvido pelo nome": bool(re.search(
            r"FRAMEWEB: titlebar_icon window_id=1 source=nome name=foot", log)),
        "foot sem aviso de toplevel icon": (
            "compositor does not implement the XDG toplevel icon protocol" not in foot_err
        ),
        "sem erro de protocolo": not re.search(r"error \d+:|protocol error", log, re.I),
    }
    for name, ok in checks.items():
        print(f"{'OK ' if ok else 'FALHA '} {name}")
    print(f"IPC cru: {ipc('icon 0')[:200]!r}")
    print(f"foot stderr: {foot_err!r}")
    print(f"log_cru={OUT}")
    print("VEREDITO: " + ("OK" if all(checks.values()) else "FALHA"))
    sys.exit(0 if all(checks.values()) else 1)
finally:
    cleanup()
