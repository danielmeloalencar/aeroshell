#!/usr/bin/env python3
"""Caminho de produção REAL: supervisor -> socketpair -> compositor + shell.

Sem harness in-process: o compositor e o shell são processos separados. A ponta
privilegiada do socketpair vai para o compositor via AERO_PRIVILEGED_FD e a
outra vai para o shell via WAYLAND_SOCKET. Verifica:
  (a) log do shell enumerando aero_frame_manager_v1 no registry;
  (b) painel layer-shell + S3 (input region + hit-test de clique);
  (c) ps mostrando os fds recebidos.
"""
import os
import socket
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/debug/aeroshell-compositor")
SHELL = os.path.join(ROOT, "target/debug/aeroshell-shell")

SOCK_NAME = "wayland-aero-supervisor-real"
INPUT_SOCK = f"/tmp/aero-supervisor-input-{os.getpid()}.sock"
LOG_COMP = f"/tmp/aero-supervisor-comp-{os.getpid()}.log"
LOG_SHELL = f"/tmp/aero-supervisor-shell-{os.getpid()}.log"
LOG_BACKDROP = f"/tmp/aero-supervisor-backdrop-{os.getpid()}.log"

for path in (INPUT_SOCK,):
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass

# Um único socketpair: duas pontas, uma para cada processo.
a, b = socket.socketpair()
comp_fd, shell_fd = a.fileno(), b.fileno()

host_display = os.environ.get("WAYLAND_DISPLAY", "wayland-0")
base = os.environ.copy()
base.pop("WAYLAND_SOCKET", None)
base.pop("AERO_PRIVILEGED_FD", None)

comp_env = base.copy()
comp_env.update(
    AERO_PRIVILEGED_FD=str(comp_fd),
    WAYLAND_DISPLAY=host_display,
    RUST_LOG="info",
    AERO_TEST_INPUT_SOCK=INPUT_SOCK,
)
comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK_NAME],
    env=comp_env,
    pass_fds=(comp_fd,),
    stdout=open(LOG_COMP, "w"),
    stderr=subprocess.STDOUT,
)
# O compositor transforma o fd em OwnedFd; a ponta Python pode fechar.
a.close()

sock_path = f"/run/user/1000/{SOCK_NAME}"
for _ in range(80):
    if os.path.exists(sock_path):
        break
    time.sleep(0.1)
if not os.path.exists(sock_path):
    print("FALHA: compositor não criou o socket público")
    print(open(LOG_COMP).read())
    comp.terminate()
    sys.exit(1)

shell_env = base.copy()
shell_env.update(
    WAYLAND_SOCKET=str(shell_fd),
    WAYLAND_DISPLAY=SOCK_NAME,
    GDK_BACKEND="wayland",
    LD_PRELOAD="libgtk4-layer-shell.so.0",
    AERO_SHELL_EXIT_AFTER="9",
    RUST_LOG="info",
)
shell = subprocess.Popen(
    [SHELL],
    env=shell_env,
    pass_fds=(shell_fd,),
    stdout=open(LOG_SHELL, "w"),
    stderr=subprocess.STDOUT,
)
b.close()

# --- (c) ps: confirmar fds recebidos -------------------------------------
ps = subprocess.run(
    ["ps", "-o", "pid,ppid,args", "-p", str(comp.pid), "-p", str(shell.pid)],
    capture_output=True,
    text=True,
).stdout
print("===== (c) ps: processos do fluxo real =====")
print(ps.strip())

# Esperar o painel e o registry probe.
for _ in range(120):
    shell_log = open(LOG_SHELL, errors="ignore").read()
    if "painel layer-shell apresentado" in shell_log and "registry do shell" in shell_log:
        break
    time.sleep(0.1)
shell_log = open(LOG_SHELL, errors="ignore").read()
comp_log = open(LOG_COMP, errors="ignore").read()

print("===== (a) log do shell: registry via WAYLAND_SOCKET =====")
for line in shell_log.splitlines():
    if "registry do shell" in line or "conectou via WAYLAND_SOCKET" in line:
        print(line)

print("===== (b) painel + S3 =====")
for line in shell_log.splitlines():
    if any(k in line for k in ("painel layer-shell", "WebView com fundo", "input region aplicada")):
        print(line)
for line in comp_log.splitlines():
    if any(k in line for k in ("nova layer surface", "Exclusive(48)", "TESTE ponteiro")):
        print(line)

# --- S3: clique dentro/fora da região interativa -------------------------
for _ in range(60):
    if "input region aplicada" in open(LOG_SHELL, errors="ignore").read():
        break
    time.sleep(0.1)
shell_log = open(LOG_SHELL, errors="ignore").read()
comp_log = open(LOG_COMP, errors="ignore").read()
import re
ansi = re.compile(r"\x1b\[[0-9;]*m")
shell_plain = ansi.sub("", shell_log)
comp_plain = ansi.sub("", comp_log)
m = re.findall(r"rects\s*=\s*\[?\(?(\d+),\s*(\d+),\s*(\d+),\s*(\d+)\)?", shell_plain)
if m:
    rx, ry, rw, rh = map(int, m[-1])
    gx, gy = 0, 602
    cx, cy = gx + rx + rw // 2, gy + ry + rh // 2
    ox, oy = gx + rx + rw + 200, cy
    print(f"geometry={gx},{gy} rect_interativa={rx},{ry} {rw}x{rh} dentro={cx},{cy} fora={ox},{oy}")
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(INPUT_SOCK)
    for line in (f"motion {cx} {cy}", "button press", "button release",
                 f"motion {ox} {oy}", "button press", "button release"):
        s.sendall((line + "\n").encode())
        time.sleep(0.3)
    s.close()
    time.sleep(0.5)
    comp_log = open(LOG_COMP, errors="ignore").read()
    print("===== (b) S3: hit-test do clique =====")
    for line in comp_log.splitlines():
        if "TESTE ponteiro" in line:
            print(line)

ok = True
ok &= "aero_frame_manager_v1" in shell_log and "privileged" in shell_log
ok &= "painel layer-shell apresentado" in shell_log
ok &= "nova layer surface" in comp_log
print(("OK: fluxo real validado" if ok else "FALHA: verificar logs acima"))
print(f"logs: {LOG_COMP}  {LOG_SHELL}")

shell.wait(timeout=15)
comp.terminate()
try:
    comp.wait(timeout=5)
except subprocess.TimeoutExpired:
    comp.kill()
sys.exit(0 if ok else 1)
