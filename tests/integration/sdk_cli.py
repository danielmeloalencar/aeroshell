#!/usr/bin/env python3
"""Tarefa 03: --list mostra id/título; --close fecha só a janela escolhida.

Pré-filtro automatizado. A validação visual final é feita pelo humano no nested
com o shell de frame (RUNBOOK), conforme AGENTS.md §0.9.
"""

import atexit
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOCK = f"wayland-aero-sdk-{os.getpid()}"
IPC = f"/tmp/opencode/aeroshell-sdk-{os.getpid()}.sock"
LOG = f"/tmp/opencode/aeroshell-sdk-{os.getpid()}.log"
COMP = ROOT / "target/debug/aeroshell-compositor"

for stale in (IPC, LOG):
    Path(stale).unlink(missing_ok=True)

env = os.environ.copy()
for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(AERO_WINDOW_IPC_SOCK=IPC, RUST_LOG="info")

comp = subprocess.Popen(
    [str(COMP), "--backend", "winit", "--socket", SOCK, "--size", "900x600",
     "--command", "foot & foot"],
    env=env, stdout=open(LOG, "w"), stderr=subprocess.STDOUT,
)


def cleanup():
    """Encerra SOMENTE o PID iniciado aqui (AGENTS.md §0.5)."""
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()


atexit.register(cleanup)


def fail(message):
    print(f"FALHA: {message}\n--- log do compositor ---\n{Path(LOG).read_text(errors='ignore')}", file=sys.stderr)
    sys.exit(1)


def cli(*args):
    return subprocess.run(
        ["bun", "run", "packages/runtime/src/main.ts", *args],
        cwd=ROOT, env={**env, "AERO_WINDOW_IPC_SOCK": IPC},
        capture_output=True, text=True, timeout=20,
    )


deadline = time.time() + 20
while time.time() < deadline and not Path(IPC).exists():
    time.sleep(0.1)
if not Path(IPC).exists():
    fail("socket IPC não criado")

listed = []
deadline = time.time() + 20
while time.time() < deadline:
    result = cli("--list")
    if result.returncode == 0:
        lines = [line for line in result.stdout.splitlines() if line.strip()]
        if len(lines) == 2:
            listed = lines
            break
    time.sleep(0.5)

if len(listed) != 2:
    fail(f"--list devolveu {len(listed)} janelas: {listed!r}\nstderr: {result.stderr}")

for line in listed:
    if not re.fullmatch(r"\d+\t.+", line):
        fail(f"linha sem id e título: {line!r}")

ids = [int(line.split("\t", 1)[0]) for line in listed]
if len(set(ids)) != 2:
    fail(f"ids repetidos: {ids}")
print("--list:", listed)

# foco é observável pela ordem de render (frente->tras) no log do compositor.
def render_order():
    clean = re.sub(r"\x1b\[[0-9;]*m", "", Path(LOG).read_text(errors="ignore"))
    lines = re.findall(r"ordem de render \(frente->tras\) order=(.+)$", clean, re.MULTILINE)
    return lines[-1] if lines else None


before_focus = render_order()
focused = cli("--focus", str(ids[0]))
if focused.returncode != 0:
    fail(f"--focus {ids[0]} falhou: {focused.stderr}")
deadline = time.time() + 5
while time.time() < deadline and render_order() == before_focus:
    time.sleep(0.2)
if render_order() == before_focus or before_focus is None:
    fail(f"--focus {ids[0]} não reordenou o Space: antes={before_focus!r} depois={render_order()!r}")
print(f"--focus {ids[0]}: ordem antes={before_focus} depois={render_order()}")

victim = ids[1]
closed = cli("--close", str(victim))
if closed.returncode != 0:
    fail(f"--close {victim} falhou: {closed.stderr}")

# send_close() é assíncrono: o cliente ainda pode estar mapeando a superfície
# quando a resposta "OK" chega. Esperar o unmap, sem descartar amostra.
deadline = time.time() + 10
after: list[str] = []
while time.time() < deadline:
    remaining = cli("--list")
    if remaining.returncode != 0:
        fail(f"--list após close falhou: {remaining.stderr}")
    after = [line for line in remaining.stdout.splitlines() if line.strip()]
    if len(after) == 1:
        break
    time.sleep(0.5)

if len(after) != 1 or int(after[0].split("\t", 1)[0]) != ids[0]:
    fail(f"--close {victim} não preservou só a janela {ids[0]}: {after!r}")

print(f"OK: --list mostrou 2 janelas; --close {victim} deixou só a janela {ids[0]}")
