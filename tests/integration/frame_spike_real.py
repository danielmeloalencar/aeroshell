#!/usr/bin/env python3
"""Supervisor do spike B.3: socketpair real -> compositor + frame_spike.
O zenity entra pelo socket público via --command (janela de teste, window_id=0).
"""
import os
import socket
import subprocess
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/debug/aeroshell-compositor")
SPIKE = os.path.join(ROOT, "target/debug/examples/frame_spike")

SOCK = "wayland-aero-b3"
LOG_COMP = "/tmp/b3-real-comp.log"
LOG_SPIKE = "/tmp/b3-real-spike.log"

base = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    base.pop(key, None)

a, b = socket.socketpair()
cfd, sfd = a.fileno(), b.fileno()

cenv = base.copy()
cenv.update(
    AERO_PRIVILEGED_FD=str(cfd),
    WAYLAND_DISPLAY=base.get("WAYLAND_DISPLAY", "wayland-0"),
    RUST_LOG="info",
    AERO_FRAME_TEST_MARGIN=os.environ.get("AERO_FRAME_TEST_MARGIN", "60"),
)
comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK,
     "--command", "zenity --info --text=JanelaDeTeste"],
    env=cenv, pass_fds=(cfd,),
    stdout=open(LOG_COMP, "w"), stderr=subprocess.STDOUT,
)
a.close()
for _ in range(100):
    if os.path.exists(f"/run/user/1000/{SOCK}"):
        break
    time.sleep(0.05)

# Evitar a corrida: o window_id 0 só existe depois que o zenity mapeia o toplevel.
for _ in range(150):
    if "novo toplevel xdg-surface mapeado" in open(LOG_COMP, errors="ignore").read():
        break
    time.sleep(0.1)
if "novo toplevel xdg-surface mapeado" not in open(LOG_COMP, errors="ignore").read():
    print("FALHA: zenity não mapeou antes do associate")
    comp.terminate()
    raise SystemExit(1)
print("zenity mapeado; lançando spike")

senv = base.copy()
senv.update(
    WAYLAND_SOCKET=str(sfd),
    WAYLAND_DISPLAY=SOCK,
    GDK_BACKEND="wayland",
    LD_PRELOAD="libgtk4-layer-shell.so.0",
    AERO_SPIKE_STAY="1",
)
spike = subprocess.Popen(
    [SPIKE], env=senv, pass_fds=(sfd,),
    stdout=open(LOG_SPIKE, "w"), stderr=subprocess.STDOUT,
)
b.close()

if os.environ.get("AERO_SPIKE_MANUAL"):
    print(f"MANUAL: compositor pid={comp.pid} spike pid={spike.pid}")
    print(f"logs: {LOG_COMP} {LOG_SPIKE}")
    print("NAO encerrar: usuario testa e fecha a janela do compositor.")
    raise SystemExit(0)

try:
    spike.wait(timeout=25)
except subprocess.TimeoutExpired:
    spike.kill()
time.sleep(0.5)
comp.terminate()
try:
    comp.wait(timeout=5)
except subprocess.TimeoutExpired:
    comp.kill()

print("=== SPIKE (privilegiado) ===")
for line in open(LOG_SPIKE, errors="ignore"):
    if any(k in line for k in ("SPIKE", "BLOQUEIO", "global")):
        print(line.rstrip())
print("=== COMPOSITOR (frame/layer/toplevel/closed) ===")
for line in open(LOG_COMP, errors="ignore"):
    if any(k in line for k in ("associ", "frame", "layer surface", "toplevel", "closed", "duplo")):
        print(line.rstrip())
print(f"logs: {LOG_COMP} {LOG_SPIKE}")
