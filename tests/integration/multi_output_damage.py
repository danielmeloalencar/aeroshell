#!/usr/bin/env python3
"""F1c: provar que `--outputs N` renderiza TODOS os outputs no winit.

Sob `bench` o compositor emite, por output e por frame:
  OUTPUT_FRAME t=... output=<nome> age=<0|1> damage=<none|rect>

Baseline (antes do F1c): so o primeiro output era renderizado, logo aero-1
nunca aparece. Nenhuma conclusao de desempenho: VM sem 3D.
"""
import atexit
import os
import re
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/debug/aeroshell-compositor")
FRAME = os.path.join(ROOT, "target/debug/examples/frame_webview")

N = 2
SOCK = f"wayland-aero-multi-{os.getpid()}"
LOG = f"/tmp/multi-comp-{os.getpid()}.log"

for orphan in (f"/run/user/1000/{SOCK}",):
    if os.path.exists(orphan):
        os.unlink(orphan)

subprocess.run(
    ["cargo", "build", "-p", "aeroshell-compositor", "--features", "bench"],
    cwd=ROOT, check=True, stdout=subprocess.DEVNULL,
)
subprocess.run(
    ["cargo", "build", "-p", "aeroshell-shell", "--example", "frame_webview"],
    cwd=ROOT, check=True, stdout=subprocess.DEVNULL,
)

env = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(RUST_LOG="debug")

comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK,
     "--outputs", str(N), "--size", "600x800",
     "--shell", FRAME,
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0",
     "--command", "foot & foot"],
    env=env, stdout=open(LOG, "w"), stderr=subprocess.STDOUT,
)


def cleanup():
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()


atexit.register(cleanup)


def log():
    return re.sub(r"\x1b\[[0-9;]*m", "", open(LOG, errors="ignore").read())


def wait_for(label, predicate, timeout_s):
    end = time.monotonic() + timeout_s
    while time.monotonic() < end:
        if comp.poll() is not None:
            print(f"FALHA: compositor morreu esperando {label}", file=sys.stderr)
            return False
        if predicate():
            return True
        time.sleep(0.25)
    print(f"FALHA: timeout esperando {label}", file=sys.stderr)
    return False


problems = []
wait_for("OUTPUT_FRAME com cliente (elements>0)", lambda: any(
    "elements=" in line and not line.endswith("elements=0")
    for line in re.findall(r"OUTPUT_FRAME ([^\n]+)", log())
), 40)
wait_for("OUTPUT_FRAME dos 2 outputs", lambda: all(
    f"output=aero-{i} " in " ".join(re.findall(r"OUTPUT_FRAME ([^\n]+)", log()))
    for i in range(N)
), 45)

lines = re.findall(r"OUTPUT_FRAME ([^\n]+)", log())
valid = [line for line in lines if "damage=" in line and "age=" in line]
print(f"OUTPUT_FRAME linhas={len(lines)} validas={len(valid)} malformadas={len(lines)-len(valid)}")
for line in valid[:8]:
    print("  ", line)

seen = {i: [line for line in valid if f"output=aero-{i} " in line] for i in range(N)}
for i in range(N):
    print(f"output aero-{i}: {len(seen[i])} frames")
    if not seen[i]:
        problems.append(f"output aero-{i} nunca renderizou")
ages = {i: sorted({re.search(r"age=(\d+)", l).group(1) for l in seen[i] if "age=" in l}) for i in range(N)}
print("ages por output:", ages)
for i in range(N):
    if ages[i] != ["0", "1"]:
        problems.append(f"output aero-{i}: esperado age 0 e 1, medido {ages[i]}")

print(f"LOG_COMPOSITOR={LOG}")
if problems:
    print(f"FALHA: {len(problems)} problema(s)")
    for p in problems:
        print("  " + p)
    print("VEREDITO: FALHA")
    sys.exit(1)
print("VEREDITO: OK")
