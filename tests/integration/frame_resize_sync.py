#!/usr/bin/env python3
"""02c: medir a defasagem entre o conteúdo e a decoração durante um resize.

O compositor é compilado com `--features bench`, que registra:
  RESIZE_SYNC_MOTION  t=<ns> window_id=<id> want=<W>x<H>     (passo do grab)
  RESIZE_SYNC_CLIENT  t=<ns> size=<W>x<H>                    (commit do conteúdo)
  RESIZE_SYNC_FRAME   t=<ns> window_id=<id> buffer=<W>x<H> content=<W>x<H>
                                                             (commit da decoração)

Duas medições independentes:
  1. lag_ms = t_frame - t_cliente, Pareado pelo tamanho de conteúdo (p95 ≤ 300 ms).
  2. passos_com_frame_pendente_no_proximo_motion: para cada passo de motion com
     tamanho novo, o frame deve ter commitado ESSE tamanho antes do próximo passo.

VM sem 3D: números são relativos, não comparáveis com GPU real. Nenhuma amostra
é descartada; todas as contagens e os primeiros valores são impressos.
"""
import atexit
import os
import re
import socket
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/debug/aeroshell-compositor")
FRAME = os.path.join(ROOT, "target/debug/examples/frame_webview")

SOCK = f"wayland-aero-sync-{os.getpid()}"
LOG = f"/tmp/sync-comp-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/sync-input-{os.getpid()}.sock"
STEPS = 14
STEP_PX = 10
MOTION_DELAY = 0.05
MAX_LAG_MS = 300.0

for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)
time.sleep(1)

# As medições só existem com a feature bench (AGENTS.md: variáveis/knobs de bench).
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
env.update(RUST_LOG="debug", AERO_TEST_INPUT_SOCK=INPUT_SOCK)

comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK,
     "--command", "foot", "--shell", FRAME,
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
    env=env, stdout=open(LOG, "w"), stderr=subprocess.STDOUT,
)


def cleanup():
    """Encerra apenas o PID iniciado aqui."""
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()


atexit.register(cleanup)


def log():
    return re.sub(r"\x1b\[[0-9;]*m", "", open(LOG, errors="ignore").read())


for _ in range(200):
    if os.path.exists(INPUT_SOCK):
        break
    time.sleep(0.05)
else:
    print("FALHA: socket de input sintético não apareceu", file=sys.stderr)
    sys.exit(1)

content = None
for _ in range(150):
    m = re.findall(
        r"frame configure \(conteúdo do cliente\).*?content_w=(\d+).*?content_h=(\d+)",
        log(),
    )
    if m:
        content = (int(m[-1][0]), int(m[-1][1]))
        break
    time.sleep(0.1)
if not content:
    print("FALHA: configure do frame não chegou", file=sys.stderr)
    print(log()[-2000:], file=sys.stderr)
    sys.exit(1)

INSETS = (6, 30, 6, 6)
content_origin = (60, 80)
frame_origin = (content_origin[0] - INSETS[0], content_origin[1] - INSETS[1])
total = (content[0] + INSETS[0] + INSETS[2], content[1] + INSETS[1] + INSETS[3])
edge_x = frame_origin[0] + total[0] - 3
mid_y = frame_origin[1] + total[1] // 2
print(f"conteudo={content} total={total} borda_direita=({edge_x},{mid_y})")
print(f"parametros: passos={STEPS} delta={STEP_PX}px delay={MOTION_DELAY:.3f}s")

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(INPUT_SOCK)


def send(line, delay=MOTION_DELAY):
    s.sendall((line + "\n").encode())
    time.sleep(delay)


send(f"motion {edge_x} {mid_y}")
send("button press", 0.1)
for i in range(STEPS):
    send(f"motion {edge_x + (i + 1) * STEP_PX} {mid_y}")
send("button release", 0.1)
time.sleep(1.2)

text = log()
motions = [
    (int(t), int(w), int(h))
    for t, w, h in re.findall(
        r"RESIZE_SYNC_MOTION t=(\d+) window_id=(?:Some\(\d+\)|\d+) want=(\d+)x(\d+)", text
    )
]
clients = [
    (int(t), int(w), int(h))
    for t, w, h in re.findall(r"RESIZE_SYNC_CLIENT t=(\d+) size=(\d+)x(\d+)", text)
]
frames = [
    (int(t), int(w), int(h))
    for t, w, h in re.findall(
        r"RESIZE_SYNC_FRAME t=(\d+) window_id=\d+ buffer=\d+x\d+ content=(\d+)x(\d+)",
        text,
    )
]

# Deduplica passos com o mesmo tamanho (motion repetido não é passo novo).
steps = []
for t, w, h in motions:
    if not steps or (w, h) != steps[-1][1:]:
        steps.append((t, w, h))

first_motion = steps[0][0] if steps else 0
clients = [c for c in clients if c[0] >= first_motion]
frames = [f for f in frames if f[0] >= first_motion]

lags = []
for ft, fw, fh in frames:
    prior = [c for c in clients if c[1] == fw and c[2] == fh and c[0] <= ft]
    if prior:
        lags.append((ft - prior[-1][0]) / 1e6)

# O foot ARREDONDA o tamanho pedido (want=706 -> conteúdo real=702). O alvo da
# decoração é o tamanho REAL commitado pelo cliente, não o `want` do motion.
unique_clients = []
for c in clients:
    if not unique_clients or c[1:] != unique_clients[-1][1:]:
        unique_clients.append(c)

pending = []
for t, w, h in unique_clients:
    deadline = next((m[0] for m in steps if m[0] > t), float("inf"))
    if not any(f[1] == w and f[2] == h and t <= f[0] < deadline for f in frames):
        pending.append((w, h))

p95 = sorted(lags)[min(len(lags) - 1, int(0.95 * (len(lags) - 1) + 0.5))] if lags else float("nan")

print(f"\nmotions={len(motions)} passos_distintos={len(steps)} "
      f"commits_cliente={len(clients)} commits_frame={len(frames)}")
print(f"primeiros passos: {steps[:4]}")
print(f"primeiros commits cliente: {clients[:4]}")
print(f"primeiros commits frame:  {frames[:4]}")
print(f"lags_ms (todos): {[round(v, 2) for v in lags]}")
print(f"lag p95={p95:.2f} ms  (limite {MAX_LAG_MS:.0f} ms)")
print(f"tamanhos REAIS do cliente: {[c[1:] for c in unique_clients[:8]]}")
print(f"commits de conteúdo com frame pendente no próximo motion: "
      f"{len(pending)}/{len(unique_clients)} {pending[:6]}")
print("\n--- trecho cru dos logs de sincronização ---")
print("\n".join(
    l for l in text.splitlines() if "RESIZE_SYNC_" in l
)[-2500:])

s.close()
cleanup()

verdict = (bool(lags) and p95 <= MAX_LAG_MS and not pending
           and len(steps) >= 6 and len(unique_clients) >= 6)
print(f"\nVEREDITO: {'OK' if verdict else 'FALHA'}")
print("(esperado: p95 ≤ 300 ms, zero commits de conteúdo pendentes, ≥6 passos e ≥6 tamanhos reais)")
sys.exit(0 if verdict else 1)
