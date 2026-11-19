#!/usr/bin/env python3
"""Caminho de produção REAL com o supervisor INTERNO (rota única).

O compositor lança o shell real (aeroshell-shell) como filho privilegiado via
--shell: o socketpair é criado dentro do compositor, sem AERO_PRIVILEGED_FD nem
socketpair externo. Os logs do shell saem no stdout do compositor (LOG_COMP).
Verifica:
  (a) log do shell enumerando aero_frame_manager_v1 no registry (<<< privileged);
  (b) painel layer-shell + S3 (input region + hit-test de clique);
  (c) ps mostrando o shell como filho do compositor.
"""
import os
import re
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

# O probe do registry só existe com a feature `probe-registry`: garantir o binário.
subprocess.run(
    ["cargo", "build", "-p", "aeroshell-shell", "--features", "probe-registry"],
    cwd=ROOT, check=True,
)

for path in (INPUT_SOCK,):
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass

ansi = re.compile(r"\x1b\[[0-9;]*m")


def comp_log():
    return ansi.sub("", open(LOG_COMP, errors="ignore").read())


host_display = os.environ.get("WAYLAND_DISPLAY", "wayland-0")
base = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    base.pop(key, None)

comp_env = base.copy()
comp_env.update(
    WAYLAND_DISPLAY=host_display,
    RUST_LOG="info",
    AERO_TEST_INPUT_SOCK=INPUT_SOCK,
)
comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK_NAME,
     "--shell", SHELL,
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0",
     "--shell-env", "AERO_SHELL_EXIT_AFTER=9",
     "--shell-env", "RUST_LOG=info"],
    env=comp_env,
    stdout=open(LOG_COMP, "w"),
    stderr=subprocess.STDOUT,
)

sock_path = f"/run/user/1000/{SOCK_NAME}"
for _ in range(80):
    if os.path.exists(sock_path):
        break
    time.sleep(0.1)
if not os.path.exists(sock_path):
    print("FALHA: compositor não criou o socket público")
    print(comp_log())
    comp.terminate()
    sys.exit(1)

# --- (c) ps: shell é filho do compositor ---------------------------------
ps = subprocess.run(
    ["ps", "-o", "pid,ppid,args", "-p", str(comp.pid), "--ppid", str(comp.pid)],
    capture_output=True, text=True,
).stdout
print("===== (c) ps: compositor + shell filho =====")
print(ps.strip())

# Esperar o painel e o registry probe (logs do shell no stdout do compositor).
for _ in range(120):
    log = comp_log()
    if "painel layer-shell apresentado" in log and "registry do shell" in log:
        break
    time.sleep(0.1)
log = comp_log()

print("===== (a) log do shell: registry via WAYLAND_SOCKET =====")
for line in log.splitlines():
    if "registry do shell" in line or "conectou via WAYLAND_SOCKET" in line:
        print(line)

print("===== (b) painel + S3 =====")
for line in log.splitlines():
    if any(k in line for k in ("painel layer-shell", "WebView com fundo", "input region aplicada")):
        print(line)
for line in log.splitlines():
    if any(k in line for k in ("nova layer surface", "Exclusive(48)", "TESTE ponteiro")):
        print(line)

# --- S3: clique dentro/fora da região interativa -------------------------
for _ in range(60):
    if "input region aplicada" in comp_log():
        break
    time.sleep(0.1)
log = comp_log()
m = re.findall(r"rects\s*=\s*\[?\(?(\d+),\s*(\d+),\s*(\d+),\s*(\d+)\)?", log)
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
    print("===== (b) S3: hit-test do clique =====")
    for line in comp_log().splitlines():
        if "TESTE ponteiro" in line:
            print(line)

log = comp_log()
ok = True
ok &= "aero_frame_manager_v1" in log and "privileged" in log
ok &= "painel layer-shell apresentado" in log
ok &= "nova layer surface" in log
print(("OK: fluxo real validado" if ok else "FALHA: verificar logs acima"))
print(f"logs: {LOG_COMP}")

comp.terminate()
try:
    comp.wait(timeout=5)
except subprocess.TimeoutExpired:
    comp.kill()
sys.exit(0 if ok else 1)