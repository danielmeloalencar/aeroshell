#!/usr/bin/env python3
"""Sobe o fluxo real de produção e DEIXA RODANDO para teste manual.

Rota única: o compositor lança o shell privilegiado (--shell) e o backdrop como
cliente PÚBLICO (--command, com env inline). O socketpair é interno. Nao injeta
cliques e nao encerra ninguem: o usuario testa e fecha a janela do compositor.
Todos os logs (compositor + shell + backdrop) saem em LOG_COMP.
"""
import os
import subprocess
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/debug/aeroshell-compositor")
SHELL = os.path.join(ROOT, "target/debug/aeroshell-shell")

SOCK_NAME = "wayland-aero-manual"
LOG_COMP = "/tmp/aero-manual-comp.log"

base = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    base.pop(key, None)
host_display = base.get("WAYLAND_DISPLAY", "wayland-0")

comp_env = base.copy()
comp_env.update(
    WAYLAND_DISPLAY=host_display,
    RUST_LOG="info",
)
comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK_NAME,
     "--shell", SHELL,
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0",
     "--shell-env", "RUST_LOG=info",
     "--command",
     f"LD_PRELOAD=libgtk4-layer-shell.so.0 AERO_TEST_BACKDROP=1 {SHELL}"],
    env=comp_env,
    stdout=open(LOG_COMP, "w"),
    stderr=subprocess.STDOUT,
    start_new_session=True,
)

sock_path = f"/run/user/1000/{SOCK_NAME}"
for _ in range(80):
    if os.path.exists(sock_path):
        break
    time.sleep(0.1)

print(f"compositor pid={comp.pid} (shell + backdrop sao filhos)")
print(f"logs: {LOG_COMP}")
print("AMBIENTE NO AR — nao encerrar automaticamente.")