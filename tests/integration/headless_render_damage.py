#!/usr/bin/env python3
"""F1b: o backend headless deve RENDERIZAR (EGL surfaceless) com damage e pixels.

Antes da F1b o headless só criava o output (`backend/headless.rs`): nenhuma
linha HEADLESS_FRAME/PIXEL_FRAME existia. Este harness é o teste-primeiro:
no baseline ele FALHA por timeout; depois da implementação exige:
  - >=3 HEADLESS_FRAME output=<nome> damage=<rect|none>
  - >=1 RENDER_SHARED window_id=<id> kind=<frame|placeholder> (mesma API do winit)
  - >=1 PIXEL_FRAME e um dump frame_*.bin de 64*40*3 bytes com conteudo nao nulo

VM sem 3D (llvmpipe): prova de funcionamento/geometria, nao de desempenho.
Nenhuma amostra e descartada: as linhas cruas relevantes sao impressas.
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

SOCK = f"wayland-aero-headless-{os.getpid()}"
LOG = f"/tmp/headless-comp-{os.getpid()}.log"
PIXELS = f"/tmp/headless-pixels-{os.getpid()}"

for orphan in (f"/run/user/1000/{SOCK}",):
    if os.path.exists(orphan):
        os.unlink(orphan)
os.makedirs(PIXELS, exist_ok=True)
time.sleep(1)

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
env.update(RUST_LOG="info", AERO_PIXEL_DUMP_DIR=PIXELS)
comp = subprocess.Popen(
    [COMP, "--backend", "headless", "--socket", SOCK,
     "--shell", FRAME, "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0",
     "--command", "foot"],
    env=env, stdout=open(LOG, "w"), stderr=subprocess.STDOUT,
)


@atexit.register
def cleanup():
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()


def log():
    return re.sub(r"\x1b\[[0-9;]*m", "", open(LOG, errors="ignore").read())


def wait_for(pattern, timeout_s=20.0, label="condição"):
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if re.search(pattern, log()):
            return True
        time.sleep(0.1)
    print(f"FALHA: timeout esperando {label}", file=sys.stderr)
    return False


# Sem render no baseline, este wait ja falha aqui (teste-primeiro).
if not wait_for(r"HEADLESS_FRAME output=", label="HEADLESS_FRAME"):
    sys.exit(1)
if not wait_for(r"RENDER_SHARED window_id=\d+ kind=", label="RENDER_SHARED"):
    sys.exit(1)
if not wait_for(r"PIXEL_FRAME t=", label="PIXEL_FRAME"):
    sys.exit(1)
time.sleep(2.0)  # acumula alguns frames para as contagens

text = log()
frames = [l for l in text.splitlines() if "HEADLESS_FRAME output=" in l]
shared = [l for l in text.splitlines() if "RENDER_SHARED window_id=" in l]
pixels = [l for l in text.splitlines() if "PIXEL_FRAME t=" in l]

print("--- HEADLESS_FRAME (primeiras 5, cruas) ---")
for l in frames[:5]:
    print("  " + l)
print("--- RENDER_SHARED (cruas) ---")
for l in shared[:5]:
    print("  " + l)
print("--- PIXEL_FRAME (cruas) ---")
for l in pixels[:5]:
    print("  " + l)

problems = []
if len(frames) < 3:
    problems.append(f"apenas {len(frames)} HEADLESS_FRAME (minimo 3)")
if not any("damage=" in l for l in frames):
    problems.append("nenhuma HEADLESS_FRAME com campo damage=")
if not shared:
    problems.append("nenhuma RENDER_SHARED (render compartilhado nao provado)")
if not any(re.search(r"kind=(frame|placeholder)\b", l) for l in shared):
    problems.append("RENDER_SHARED sem kind=frame|placeholder")
if not pixels:
    problems.append("nenhuma PIXEL_FRAME")

dumps = sorted(
    f for f in os.listdir(PIXELS) if re.fullmatch(r"frame_\d{6}\.bin", f)
)
nonzero = 0
for name in dumps:
    data = open(os.path.join(PIXELS, name), "rb").read()
    if len(data) != 64 * 40 * 3:
        problems.append(f"{name}: {len(data)} bytes (esperado {64*40*3})")
        continue
    if any(data):
        nonzero += 1
print(f"\ndumps={len(dumps)} com_conteudo_nao_nulo={nonzero} (dir={PIXELS})")
if not dumps:
    problems.append("nenhum dump frame_*.bin da sonda 02o")
elif nonzero == 0:
    problems.append("todos os dumps sao 64x40x3 zerados (render sem conteudo)")

print(f"\nHEADLESS_FRAME={len(frames)} RENDER_SHARED={len(shared)} PIXEL_FRAME={len(pixels)}")
if problems:
    for p in problems:
        print("  FALHA: " + p)
    print("VEREDITO: FALHA")
    sys.exit(1)
print("VEREDITO: OK")
