#!/usr/bin/env python3
"""F2-compositor-cursor-shape: o compositor aplica a forma pedida pelo cliente.

Pré-requisito (check.sh reconstrói o binário SEM bench; reconstrua antes):
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-shell --example frame_webview
  cargo build -p aeroshell-shell --example cursor_shape_client

Roda no nested (precisa de DISPLAY); fecha somente os PIDs que iniciou.
Uso: python3 tests/integration/f2_compositor_cursor_shape.py
"""
import os
import pathlib
import re
import socket
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
COMP_BIN = ROOT / "target/debug/aeroshell-compositor"
if not COMP_BIN.exists() or b"CURSOR_APPLY" not in COMP_BIN.read_bytes():
    print("PRÉ-REQUISITO: reconstrua o compositor com bench antes deste harness:")
    print("  cargo build -p aeroshell-compositor --features bench")
    sys.exit(2)
OUT = pathlib.Path("/tmp/opencode/f2-cursor-shape")
OUT.mkdir(parents=True, exist_ok=True)
SOCK = f"wayland-aero-cursor-{os.getpid()}"
LOG = OUT / f"comp-{os.getpid()}.log"
IPC = OUT / f"ipc-{os.getpid()}.sock"
INPUT = OUT / f"input-{os.getpid()}.sock"

for orphan in (f"/run/user/1000/{SOCK}", str(IPC), str(INPUT)):
    if os.path.exists(orphan):
        os.unlink(orphan)

env = os.environ.copy()
for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(
    RUST_LOG="debug",
    AERO_WINDOW_IPC_SOCK=str(IPC),
    AERO_TEST_INPUT_SOCK=str(INPUT),
    AERO_CURSOR_SHAPE_SEQUENCE="wait,text",
)

comp = subprocess.Popen(
    [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "winit",
     "--socket", SOCK, "--size", "900x600",
     "--command", str(ROOT / "target/debug/examples/cursor_shape_client"),
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


def send(line):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(3)
        s.connect(str(INPUT))
        s.sendall(f"{line}\n".encode())


def wait_for(marker, timeout=20.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if marker in log():
            return True
        if comp.poll() is not None:
            return False
        time.sleep(0.1)
    return False


def geometry():
    for line in ipc("list").splitlines():
        if line.startswith("W 0 "):
            return tuple(int(v) for v in line.split()[2:6])
    return None


failures = []
try:
    if not wait_for("CURSOR_CLIENT pronto shape=wait", 30.0):
        failures.append("cliente GTK não pediu cursor")
    if not wait_for("FRAMEWEB: associate(window_id=0)", 30.0):
        failures.append("shell não associou o frame")

    #O GDK só envia set_shape quando a superfície tem foco de ponteiro: sem
    #isso o cliente muda o cursor localmente e o compositor nada recebe.
    geo = geometry()
    print(f"geometria={geo}")
    if geo is None:
        failures.append("sem geometria da janela no IPC")
    else:
        x, y, w, h = geo
        send(f"motion {x + w // 2} {y + h // 2}")
        time.sleep(0.3)

    #Pedido do cliente: chega ao compositor (CURSOR_SHAPE) e precisa ser APLICADO.
    #O CURSOR_APPLY só é emitido em redraw; sem atividade o pedido fica pendente
    #na forma e o próximo ciclo do cliente o substitui. Forçamos um motion.
    for shape in ("wait", "text"):
        arrived = wait_for(f"source=named shape={shape}", 20.0)
        if geo is not None:
            send(f"motion {x + w // 2} {y + h // 2}")
        applied = False
        deadline = time.time() + 3.0
        while time.time() < deadline:
            if re.search(rf"CURSOR_APPLY t=\d+ shape={shape}\b", log()):
                applied = True
                break
            time.sleep(0.1)
        print(f"cliente shape={shape} recebido={arrived} aplicado={applied}")
        if not arrived:
            failures.append(f"pedido {shape} não chegou ao compositor")
        if not applied:
            failures.append(f"pedido {shape} não foi aplicado no backend winit")

    #Moldura SSD: o shell pede all-scroll na titlebar e ns-resize na borda baixa.
    if geo is None:
        failures.append("sem geometria da janela no IPC (SSD)")
    else:
        x, y, w, h = geo
        # WebKit traduz `cursor:all-scroll` do CSS para o shape `move` do protocolo.
        # Cada ponto é medido com janela de 3 s e motion repetido: sem redraw o
        # CURSOR_APPLY não é emitido (mesma lacuna da parte do cliente).
        for shape, point in (
            ("move", (x + w // 2, y - 15)),
            ("ns-resize", (x + w // 2, y + h + 3)),
        ):
            asked = False
            applied = False
            deadline = time.time() + 3.0
            while time.time() < deadline:
                send(f"motion {point[0]} {point[1]}")
                time.sleep(0.2)
                text = log()
                asked |= bool(re.search(rf"CURSOR_SHAPE t=\d+ source=named shape={shape}\b", text))
                if re.search(rf"CURSOR_APPLY t=\d+ shape={shape}\b", text):
                    applied = True
                    break
            print(f"ssd shape={shape} pedido_pelo_shell={asked} aplicado={applied}")
            if not asked:
                failures.append(f"shell não pediu {shape} na moldura")
            if not applied:
                failures.append(f"{shape} da moldura não foi aplicado")

    text = log()
    print("--- sondas (amostra crua) ---")
    for line in text.splitlines():
        if "CURSOR_CLIENT" in line or "CURSOR_SHAPE" in line or "CURSOR_APPLY" in line:
            print(line)
    print("--- veredito ---")
    if failures:
        print("VEREDITO: FALHA")
        for failure in failures:
            print(f"  - {failure}")
        sys.exit(1)
    print("VEREDITO: OK")
finally:
    cleanup()
