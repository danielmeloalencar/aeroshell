#!/usr/bin/env python3
"""Sobe o fluxo real de produção e DEIXA RODANDO para teste manual.

Supervisor -> socketpair -> compositor (AERO_PRIVILEGED_FD) + shell (WAYLAND_SOCKET)
+ backdrop (socket publico). Nao injeta cliques e nao encerra ninguem: o usuario
testa e fecha a janela do compositor quando quiser.
"""
import os
import socket
import subprocess
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/debug/aeroshell-compositor")
SHELL = os.path.join(ROOT, "target/debug/aeroshell-shell")

SOCK_NAME = "wayland-aero-manual"
LOG_COMP = "/tmp/aero-manual-comp.log"
LOG_SHELL = "/tmp/aero-manual-shell.log"
LOG_BACKDROP = "/tmp/aero-manual-backdrop.log"

a, b = socket.socketpair()
comp_fd, shell_fd = a.fileno(), b.fileno()

base = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    base.pop(key, None)
host_display = base.get("WAYLAND_DISPLAY", "wayland-0")

comp_env = base.copy()
comp_env.update(
    AERO_PRIVILEGED_FD=str(comp_fd),
    WAYLAND_DISPLAY=host_display,
    RUST_LOG="info",
)
comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK_NAME],
    env=comp_env,
    pass_fds=(comp_fd,),
    stdout=open(LOG_COMP, "w"),
    stderr=subprocess.STDOUT,
    start_new_session=True,
)
a.close()

sock_path = f"/run/user/1000/{SOCK_NAME}"
for _ in range(80):
    if os.path.exists(sock_path):
        break
    time.sleep(0.1)

shell_env = base.copy()
shell_env.update(
    WAYLAND_SOCKET=str(shell_fd),
    WAYLAND_DISPLAY=SOCK_NAME,
    GDK_BACKEND="wayland",
    LD_PRELOAD="libgtk4-layer-shell.so.0",
    RUST_LOG="info",
)
shell = subprocess.Popen(
    [SHELL],
    env=shell_env,
    pass_fds=(shell_fd,),
    stdout=open(LOG_SHELL, "w"),
    stderr=subprocess.STDOUT,
    start_new_session=True,
)
b.close()

back_env = base.copy()
back_env.update(
    WAYLAND_DISPLAY=SOCK_NAME,
    GDK_BACKEND="wayland",
    LD_PRELOAD="libgtk4-layer-shell.so.0",
    AERO_TEST_BACKDROP="1",
    RUST_LOG="info",
)
backdrop = subprocess.Popen(
    [SHELL],
    env=back_env,
    stdout=open(LOG_BACKDROP, "w"),
    stderr=subprocess.STDOUT,
    start_new_session=True,
)

print(f"compositor pid={comp.pid}  shell pid={shell.pid}  backdrop pid={backdrop.pid}")
print(f"logs: {LOG_COMP}  {LOG_SHELL}  {LOG_BACKDROP}")
print("AMBIENTE NO AR — nao encerrar automaticamente.")
