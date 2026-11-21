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
    # F1c: exigir o branch 9-slice (buffer real do WebKit), nao so placeholder.
    wait_for("RENDER_SHARED kind=9slice", lambda: "kind=9slice" in log(), 60)
    # F1c: exigir damage incremental (none em frame estavel) e repaint parcial.
    wait_for("damage=none", lambda: "damage=none" in log(), 20)

all_headless = re.findall(r"HEADLESS_FRAME ([^\n]+)", log())
# Linhas intercaladas com stdout do shell (FRAMEWEB: ...) sao registradas como
# malformadas, nunca usadas como amostra e nunca descartadas em silencio.
frames = [line for line in all_headless if "damage=" in line]
malformed_headless = len(all_headless) - len(frames)
pixels = re.findall(r"PIXEL_FRAME ([^\n]+)", log())
shared = re.findall(r"RENDER_SHARED ([^\n]+)", log())
print(f"HEADLESS_FRAME linhas: {len(all_headless)} validas={len(frames)} malformadas={malformed_headless}")
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
nines = [line for line in shared if "kind=9slice" in line]
if not nines:
    problems.append("nenhum RENDER_SHARED kind=9slice: branch 9-slice nao exercitado no headless")
if not any("damage=none" in line for line in frames):
    problems.append("nenhum damage=none: damage incremental nao provado")

# Tamanho do output vem do proprio log (nao supor 1280x800).
m_size = re.search(r"size=(\d+)x(\d+)", pixels[0]) if pixels else None
PIXEL_W, PIXEL_H = (int(m_size.group(1)), int(m_size.group(2))) if m_size else (1280, 800)
print(f"output medido: {PIXEL_W}x{PIXEL_H}")

# Medir (nao descartar): sequencia COMPLETA de damage e grid de pixels real.
damages = [re.search(r"damage=(\S+)", line).group(1) for line in frames if "damage=" in line]
print("damage total=", len(damages), "contagem=", dict((d, damages.count(d)) for d in set(damages)))
print("damage sequence (completa):", damages)
partial = [d for d in damages if d not in ("none",) and not d.startswith(f"{PIXEL_W}x{PIXEL_H}@0,0")]
print(f"damage parcial (menor que o output): {len(partial)} -> {partial[:8]}")
if partial:
    print("PROVA_DAMAGE_INCREMENTAL: sim")
else:
    problems.append("nenhum damage parcial medido (so full/none)")

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

    # F1c: comparar a REGIAO DO FRAME do grid capturado com 9-slice contra o
    # grid baselines (clear color). Fora do frame nao pode mudar.
    if nines and len(grids) >= 2:
        mrect = re.search(r"x=(-?\d+) y=(-?\d+) w=(\d+) h=(\d+)", nines[-1])
        if mrect:
            fx, fy, fw, fh = (int(mrect.group(i)) for i in (1, 2, 3, 4))
            cw, ch = PIXEL_W / 64.0, PIXEL_H / 40.0
            x0, x1 = max(0, int(fx / cw)), min(64, int((fx + fw) / cw) + 1)
            y0, y1 = max(0, int(fy / ch)), min(40, int((fy + fh) / ch) + 1)
            base = open(grids[0], "rb").read()
            last = open(grids[-1], "rb").read()
            diff_in = diff_out = 0
            for cy in range(40):
                for cx in range(64):
                    off = (cy * 64 + cx) * 3
                    d = sum(abs(base[off + k] - last[off + k]) for k in range(3))
                    if x0 <= cx < x1 and y0 <= cy < y1:
                        diff_in += d
                    else:
                        diff_out += d
            print(f"frame_rect=({fx},{fy},{fw},{fh}) celulas={x1-x0}x{y1-y0} "
                  f"diff_dentro={diff_in} diff_fora={diff_out}")
            if diff_in <= 0:
                problems.append("regiao do frame igual ao clear color (9-slice nao apareceu nos pixels)")
        else:
            problems.append("RENDER_SHARED 9slice sem geometria x/y/w/h")
    elif not nines:
        pass  # ja registrado acima
    else:
        problems.append("menos de 2 grids capturados; comparacao de regiao impossivel")

print(f"LOG_COMPOSITOR={LOG}")
if problems:
    print(f"FALHA: {len(problems)} problema(s)")
    for problem in problems:
        print("  " + problem)
    print("VEREDITO: FALHA")
    sys.exit(1)
print("VEREDITO: OK")
