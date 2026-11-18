#!/usr/bin/env python3
"""Supervisor do spike B.3: socketpair real -> compositor + frame_spike.

Gate de tamanho (B.3): FRAME_BUFFER_COMMIT buffer == expected (conteúdo +
insets + shadow) em DOIS tamanhos diferentes — o cliente azul se redimensiona
sozinho (AERO_SPIKE_RESIZE) para provar que o frame SEGUE resize.
Depois mata o cliente azul para provar closed -> frame destrói.
"""
import os
import re
import socket
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/debug/aeroshell-compositor")
SPIKE = os.path.join(ROOT, "target/debug/examples/frame_spike")

SOCK = "wayland-aero-b3"
LOG_COMP = "/tmp/b3-real-comp.log"
LOG_SPIKE = "/tmp/b3-real-spike.log"

# insets do spike: (0, 30, 0, 0); shadow: 0.
SIZE_1 = (320, 270)   # cliente 320x240 + titlebar 30
SIZE_2 = (480, 350)   # cliente 480x320 + titlebar 30

base = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    base.pop(key, None)

a, b = socket.socketpair()
cfd, sfd = a.fileno(), b.fileno()

cenv = base.copy()
cenv.update(
    AERO_PRIVILEGED_FD=str(cfd),
    WAYLAND_DISPLAY=base.get("WAYLAND_DISPLAY", "wayland-0"),
    RUST_LOG="debug",
)
comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK,
     "--command", f"AERO_SPIKE_BLUE_CLIENT=1 AERO_SPIKE_RESIZE=480x320 {SPIKE}"],
    env=cenv, pass_fds=(cfd,),
    stdout=open(LOG_COMP, "w"), stderr=subprocess.STDOUT,
)
a.close()
for _ in range(100):
    if os.path.exists(f"/run/user/1000/{SOCK}"):
        break
    time.sleep(0.05)


def comp_log():
    return re.sub(r"\x1b\[[0-9;]*m", "", open(LOG_COMP, errors="ignore").read())


def fail(msg):
    print(f"FALHA: {msg}")
    comp.terminate()
    try:
        spike.wait(timeout=3)
    except Exception:
        spike.kill()
    try:
        comp.wait(timeout=5)
    except subprocess.TimeoutExpired:
        comp.kill()
    print(f"logs: {LOG_COMP} {LOG_SPIKE}")
    sys.exit(1)


# Evitar a corrida: o window_id 0 só existe depois que o cliente mapeia o toplevel.
for _ in range(150):
    if "novo toplevel xdg-surface mapeado" in comp_log():
        break
    time.sleep(0.1)
if "novo toplevel xdg-surface mapeado" not in comp_log():
    fail("cliente azul não mapeou antes do associate")
# Esperar o cliente azul comitar buffer (geometry 0x0 até o primeiro commit).
time.sleep(2)
print("cliente azul com buffer; lançando spike")

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

COMMIT_RE = re.compile(
    r"FRAME_BUFFER_COMMIT buffer=Some\(\((\d+), (\d+)\)\) "
    r"expected=Some\(\((\d+), (\d+)\)\) ok=(true|false)"
)


def wait_gate(size, timeout_s):
    """Espera um commit com buffer == expected == size e ok=true."""
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        for m in COMMIT_RE.finditer(comp_log()):
            buf = (int(m.group(1)), int(m.group(2)))
            exp = (int(m.group(3)), int(m.group(4)))
            if buf == size and exp == size and m.group(5) == "true":
                return m.group(0)
        time.sleep(0.2)
    return None


print(f"gate 1: esperando buffer == expected == {SIZE_1}")
line1 = wait_gate(SIZE_1, 30)
if not line1:
    fail(f"gate 1 não passou em 30 s (esperado {SIZE_1})")
print(f"OK gate 1: {line1}")

print(f"gate 2: esperando resize do cliente azul -> {SIZE_2}")
line2 = wait_gate(SIZE_2, 30)
if not line2:
    fail(f"gate 2 não passou em 30 s (esperado {SIZE_2}; frame não seguiu resize?)")
print(f"OK gate 2: {line2}")

# Prova de closed: matar o cliente azul (filho real do sh -c, não o sh).
m = re.findall(r"cliente inicial iniciado.*?pid[=: ]+(\d+)", comp_log())
if m:
    sh_pid = int(m[-1])
    children = subprocess.run(
        ["ps", "-o", "pid=", "--ppid", str(sh_pid)],
        capture_output=True, text=True,
    ).stdout.split()
    for child in children or [str(sh_pid)]:
        print(f"matando cliente azul pid={child}")
        try:
            os.kill(int(child), 15)
        except ProcessLookupError:
            pass
    time.sleep(1)

slog = open(LOG_SPIKE, errors="ignore").read()
if "closed recebido" not in slog:
    fail("spike não recebeu closed após morte do cliente")
if "janela cliente destruída: closed enviado ao frame" not in comp_log():
    fail("compositor não logou closed via hook de destruição")
print("OK closed: compositor enviou e spike recebeu")

try:
    spike.wait(timeout=3)
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
print("=== COMPOSITOR (frame/commit/closed) ===")
for line in open(LOG_COMP, errors="ignore"):
    if any(k in line for k in ("associ", "frame", "FRAME_BUFFER_COMMIT", "closed", "duplo", "reconfig")):
        print(line.rstrip())
print(f"logs: {LOG_COMP} {LOG_SPIKE}")
print("GATE DE TAMANHO: PASSOU (2 tamanhos + closed)")