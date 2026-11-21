#!/usr/bin/env python3
"""F1b: provar render compartilhado + damage no backend headless (VM sem 3D).

O compositor headless usa GlesRenderer EGL surfaceless (llvmpipe) e chama o MESMO
`render::elements::push_frame_elements` do winit. Sob `bench`:
  RENDER_SHARED window_id=<id> kind=<frame|placeholder>   — prova do modulo compartilhado
  HEADLESS_FRAME output=<nome> damage=<none|rectangle>    — OutputDamageTracker por output
  PIXEL_FRAME t=... size=WxH sha=...                      — sonda 02o (pixels reais)

Nenhuma conclusao de desempenho: VM sem 3D. Nenhuma amostra e descartada em silencio.
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
PIXELS = f"/tmp/opencode/f1b_pixels_{os.getpid()}"

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
for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(RUST_LOG="debug", AERO_PIXEL_DUMP_DIR=PIXELS, GDK_BACKEND="wayland")
os.makedirs(PIXELS, exist_ok=True)

comp = subprocess.Popen(
    [COMP, "--backend", "headless", "--socket", SOCK,
     "--shell", FRAME,
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0",
     "--command", "foot"],
    env=env, stdout=open(LOG, "w"), stderr=subprocess.STDOUT,
)


def cleanup():
    """Encerra somente o PID que este harness iniciou."""
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
            sys.exit(1)
        if predicate():
            return True
        time.sleep(0.25)
    print(f"FALHA: timeout esperando {label}", file=sys.stderr)
    return False


problems = []
ok = wait_for("HEADLESS_FRAME", lambda: log().count("HEADLESS_FRAME") >= 3, 45)
if ok:
    wait_for("PIXEL_FRAME", lambda: "PIXEL_FRAME" in log(), 20)
    wait_for("RENDER_SHARED", lambda: "RENDER_SHARED" in log(), 20)

frames = re.findall(r"HEADLESS_FRAME ([^\n]+)", log())
pixels = re.findall(r"PIXEL_FRAME ([^\n]+)", log())
shared = re.findall(r"RENDER_SHARED ([^\n]+)", log())
print("HEADLESS_FRAME linhas:", len(frames))
for line in frames[:6]:
    print("  ", line)
print("RENDER_SHARED linhas:", len(shared))
for line in shared[:6]:
    print("  ", line)
print("PIXEL_FRAME linhas:", len(pixels))
for line in pixels[:3]:
    print("  ", line)

if len(frames) < 3:
    problems.append(f"esperado >=3 HEADLESS_FRAME, medido {len(frames)}")
if len(shared) < 1:
    problems.append("nenhuma linha RENDER_SHARED: render/elements.rs nao foi reutilizado")
if len(pixels) < 1:
    problems.append("nenhuma linha PIXEL_FRAME: sonda 02o nao capturou pixels no headless")
if not all("damage=" in line for line in frames):
    problems.append("HEADLESS_FRAME sem campo damage=")

# Medir (nao descartar): sequencia de damage e grid de pixels real.
damages = [re.search(r"damage=(\S+)", line).group(1) for line in frames if "damage=" in line]
print("damage sequence:", damages[:12])

grids = sorted(
    (os.path.join(PIXELS, f) for f in os.listdir(PIXELS) if f.startswith("frame_") and f.endswith(".bin"))
)
print(f"grid files={len(grids)} dir={PIXELS}")
if grids:
    data = open(grids[0], "rb").read()
    print(f"grid[0] bytes={len(data)} nao_zero={sum(1 for b in data if b)}")
    if len(data) != 64 * 40 * 3:
        problems.append(f"grid com {len(data)} bytes, esperado {64*40*3}")
    if not any(byte for byte in data):
        problems.append("grid de pixels todo zero")

print(f"LOG_COMPOSITOR={LOG}")
if problems:
    print(f"FALHA: {len(problems)} problema(s)")
    for problem in problems:
        print("  " + problem)
    print("VEREDITO: FALHA")
    sys.exit(1)
print("VEREDITO: OK")
