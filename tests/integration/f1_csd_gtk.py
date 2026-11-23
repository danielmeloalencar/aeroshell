#!/usr/bin/env python3
"""F1-csd-gtk: cliente CSD sem barra HTML dupla e com move/resize via xdg_toplevel.

Pre-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-compositor --example csd_client
  cargo build -p aeroshell-shell --example frame_webview

Roda no nested (precisa de DISPLAY); fecha somente os PIDs que iniciou.
Uso: python3 tests/integration/f1_csd_gtk.py
"""
import os
import pathlib
import re
import socket
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
GEOMETRY_ONLY = "--geometry-only" in sys.argv
OUT = pathlib.Path("/tmp/opencode/f1-csd")
OUT.mkdir(parents=True, exist_ok=True)
SOCK = f"wayland-aero-csd-{os.getpid()}"
LOG = OUT / f"comp-{os.getpid()}.log"
IPC = OUT / f"ipc-{os.getpid()}.sock"
INPUT = OUT / f"input-{os.getpid()}.sock"
GO = OUT / f"go-{os.getpid()}"

for orphan in (f"/run/user/1000/{SOCK}", str(IPC), str(INPUT), str(GO)):
    if os.path.exists(orphan):
        os.unlink(orphan)

env = os.environ.copy()
for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(
    RUST_LOG="debug", AERO_WINDOW_IPC_SOCK=str(IPC),
    AERO_TEST_INPUT_SOCK=str(INPUT), AERO_CSD_GO=str(GO),
)
if GEOMETRY_ONLY:
    env["AERO_CSD_GEOMETRY_ONLY"] = "1"

comp = subprocess.Popen(
    [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "winit",
     "--socket", SOCK, "--size", "900x600",
     "--command", str(ROOT / "target/debug/examples/csd_client"),
     "--shell", str(ROOT / "target/debug/examples/frame_webview"),
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
    cwd=ROOT, env=env, stdout=LOG.open("wb"), stderr=subprocess.STDOUT,
)


def cleanup():
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()
            comp.wait(timeout=5)


def log():
    return re.sub(r"\x1b\[[0-9;]*m", "", LOG.read_text(errors="replace")) if LOG.exists() else ""


def ipc(cmd):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(3)
        s.connect(str(IPC))
        s.sendall(f"{cmd}\n".encode())
        s.shutdown(socket.SHUT_WR)
        return s.recv(4096).decode(errors="replace")


def geometry():
    for line in ipc("list").splitlines():
        if line.startswith("W 0 "):
            parts = line.split()
            return tuple(int(v) for v in parts[2:6])
    return None


def send(line):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.connect(str(INPUT))
        s.sendall(f"{line}\n".encode())


def wait_for(marker, timeout=25.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if marker in log():
            return True
        if comp.poll() is not None:
            return False
        time.sleep(0.1)
    return False


try:
    ready = wait_for("CSD_CLIENT pronto_para_input")
    wait_for("FRAMEWEB: associate(window_id=0)")
    before = geometry()
    print(f"pronto={ready} geometria_inicial={before}")

    if ready and before:
        cx, cy = before[0] + before[2] // 2, before[1] + before[3] // 2
        send(f"motion {cx} {cy}")
        send("button press")
        wait_for("CSD_MOVE t=", 10.0)
        wait_for("decisao=allow acao=move", 10.0)
        if not re.search(r"CSD_MOVE t=\d+ window_id=0 decisao=allow", log()):
            print("FALHA: grab de move não iniciou")
            sys.exit(1)
        send(f"motion {cx + 400} {cy + 250}")
        time.sleep(0.3)
        send("button release")
        time.sleep(0.3)
    moved = geometry()
    print(f"geometria_pos_move={moved}")
    GO.touch()

    if ready and moved:
        bx, by = moved[0] + moved[2] - 10, moved[1] + moved[3] - 10
        send(f"motion {bx} {by}")
        send("button press")
        wait_for("CSD_RESIZE t=", 10.0)
        wait_for("decisao=allow acao=resize", 10.0)
        if not re.search(r"CSD_RESIZE t=\d+ window_id=0 decisao=allow", log()):
            print("FALHA: grab de resize não iniciou")
            sys.exit(1)
        send(f"motion {bx + 200} {by + 150}")
        time.sleep(0.8)
        send("button release")
        time.sleep(0.8)
    resized = geometry()
    print(f"geometria_pos_resize={resized}")

    text = log()
    delta_loc = (
        abs(moved[0] - before[0]) + abs(moved[1] - before[1])
        if before and moved else 0
    )
    delta_size = (
        (resized[2] - before[2]) + (resized[3] - before[3])
        if before and resized else 0
    )
    checks = {
        "cliente pediu sinal CSD": (
            "CSD_CLIENT mode_requested=ClientSide" in text
            if not GEOMETRY_ONLY
            else "CSD_CLIENT mode_requested=GeometryOnly" in text
        ),
        "pedido com serial invalido": "CSD_CLIENT invalid_move_requested serial=0" in text,
        "CSD detectado (bench)": bool(re.search(r"CSD_DETECT t=\d+ window_id=0 client_frame=true", text)),
        "serial invalido recusado": bool(re.search(r"CSD_MOVE t=\d+ window_id=0 decisao=deny", text)),
        "barra HTML escondida (DOM)": bool(re.search(
            r"FRAMEWEB: csd_applied window_id=0 client_frame=true bar=hidden", text)),
        "sem bordas do frame": (
            bool(re.search(r"FRAMEWEB: associate\(window_id=0\) \+ set_insets\(0,0,0,0\)", text))
            or bool(re.search(r"FRAMEWEB: csd_late window_id=0 insets=\(0,0,0,0\)", text))
        ),
        "move pelo cliente": delta_loc >= 150,
        "resize pelo cliente": delta_size >= 80,
        "sem erro de protocolo": not re.search(r"error \d+:|protocol error", text, re.I),
    }
    for name, ok in checks.items():
        print(f"{'OK ' if ok else 'FALHA '} {name}")
    print(f"delta_loc={delta_loc} delta_size={delta_size}")
    print(f"log_cru={OUT}")
    print("VEREDITO: " + ("OK" if all(checks.values()) else "FALHA"))
    sys.exit(0 if all(checks.values()) else 1)
finally:
    cleanup()
