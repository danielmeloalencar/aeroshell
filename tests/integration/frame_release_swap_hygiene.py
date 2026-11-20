#!/usr/bin/env python3
"""02n: swap atômico no release — nenhum frame com buffer inconsistente.

Especificação do humano (2026-11-20): no release de um resize com arrasto, o
compositor deve CONTINUAR apresentando o buffer congelado via 9-slice (já correto
no target final) e só aposentar o congelado quando o buffer novo casar com a
referência/versão das slices — swap atômico num único render. O `source=final`
é adiado para idle (~150-250 ms).

Métrica objetiva (sem julgar pixels, só sequência de estados no log bench):
  - `frames_buffer_inconsistente` = frames após o `source=final` em que o tamanho
    do buffer MUDA e, no MESMO render, `rescaled` cai de true para false (o
    compositor troca 9-slice -> Surface nativo com o layout ainda mid-resize).
    Baseline medido em /tmp/gh-comp-110069.log: 1 (708x530 rescaled=true ->
    768x582 rescaled=false no mesmo frame).
  - `swaps_atomicos` = transições em que o buffer muda para o tamanho final
    ENQUANTO o 9-slice ainda está ativo (rescaled=true), e só depois o
    9-slice é aposentado (rescaled=false com buf==target). É o swap invisível.

Todas as linhas RESIZE_SLICE cruas são impressas; nenhuma amostra é descartada.
VM sem 3D: só geometria/sequência, nenhuma conclusão de desempenho.
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

SOCK = f"wayland-aero-rsw-{os.getpid()}"
LOG = f"/tmp/rsw-comp-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/rsw-input-{os.getpid()}.sock"
STEPS = 8
STEP_PX = 8
MOTION_DELAY = 0.05

for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)
time.sleep(1)

# A instrumentação RESIZE_SLICE/FRAME_CONFIGURE só existe com a feature bench.
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
    [COMP, "--backend", "winit", "--socket", SOCK, "--command", "foot",
     "--shell", FRAME, "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
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


def wait_for(pattern, timeout_s=20.0, label="condição"):
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if re.search(pattern, log()):
            return True
        time.sleep(0.1)
    print(f"FALHA: timeout esperando {label}", file=sys.stderr)
    return False


if not wait_for(r"FRAMEWEB: associate\(window_id=0\)", label="associate"):
    sys.exit(1)
if not wait_for(r"FRAME_BUFFER_COMMIT buffer=Some\(\(\d+, \d+\)\)", label="buffer inicial"):
    sys.exit(1)

config = None
m = re.findall(
    r"frame configure \(conteúdo do cliente\).*?content_w=(\d+).*?content_h=(\d+)"
    r".*?total_w=(\d+).*?total_h=(\d+)", log(),
)
if m:
    config = tuple(int(v) for v in m[-1])
if not config:
    print("FALHA: configure do frame não chegou", file=sys.stderr)
    sys.exit(1)

INSETS = (6, 30, 6, 6)
frame_origin = (60 - INSETS[0], 80 - INSETS[1])
extras = (config[2] - config[0], config[3] - config[1])
total = (config[0] + extras[0], config[1] + extras[1])
edge_x = frame_origin[0] + total[0] - 3
edge_y = frame_origin[1] + total[1] - 3
print(f"conteudo={config[:2]} total={total} canto=({edge_x},{edge_y})", flush=True)

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(INPUT_SOCK)
s.sendall(f"motion {edge_x} {edge_y}\n".encode())
time.sleep(0.2)
s.sendall(b"button press\n")
time.sleep(0.1)
for i in range(STEPS):
    s.sendall(f"motion {edge_x + (i + 1) * STEP_PX} {edge_y + (i + 1) * STEP_PX}\n".encode())
    time.sleep(MOTION_DELAY)
s.sendall(b"button release\n")
release_ns = time.time_ns()
time.sleep(1.5)

slice_pat = re.compile(
    r"RESIZE_SLICE t=(\d+) window_id=(\d+) buf=(\d+)x(\d+) target=(\d+)x(\d+) "
    r"rescaled=(true|false)"
)
slices = []
for l in log().splitlines():
    m = slice_pat.search(l)
    if m and int(m.group(2)) == 0:
        slices.append({
            "t": int(m.group(1)), "buf": (int(m.group(3)), int(m.group(4))),
            "target": (int(m.group(5)), int(m.group(6))),
            "rescaled": m.group(7) == "true", "raw": l,
        })

final_ts = [int(m.group(1)) for m in re.finditer(
    r"FRAME_CONFIGURE t=(\d+) \S+ window_id=0 \S+ \S+ source=final", log()
)]
final_t = final_ts[-1] if final_ts else release_ns
post = [x for x in slices if x["t"] >= final_t]

print(f"\n--- RESIZE_SLICE apos source=final (cruo; {len(post)} linhas) ---")
for x in post:
    dt = (x["t"] - final_t) / 1e6
    print(f"  t={dt:+9.2f}ms buf={x['buf'][0]}x{x['buf'][1]} "
          f"target={x['target'][0]}x{x['target'][1]} rescaled={x['rescaled']}")

# Buffer muda e 9-slice cai no MESMO render = frame inconsistente (baseline=1).
inconsistentes = []
atomicos = 0
for prev, cur in zip(post, post[1:]):
    buf_changed = cur["buf"] != prev["buf"]
    slice_dropped = prev["rescaled"] and not cur["rescaled"]
    buf_at_target = cur["buf"] == cur["target"]
    if buf_changed and slice_dropped:
        inconsistentes.append(cur)
    # buffer chegou ao target ainda com 9-slice ativo -> swap vai ser atômico
    if buf_changed and buf_at_target and cur["rescaled"]:
        atomicos += 1

print(f"\nframes_buffer_inconsistente={len(inconsistentes)} swaps_atomicos={atomicos} "
      f"source_final={len(final_ts)}")
problems = []
if inconsistentes:
    for x in inconsistentes:
        problems.append(
            f"buffer {x['buf'][0]}x{x['buf'][1]} com 9-slice abandonado no mesmo render "
            f"(target {x['target'][0]}x{x['target'][1]})"
        )
if atomicos < 1:
    problems.append("nenhum swap atômico: buffer nunca alcançou o target com 9-slice ativo")

print()
if problems:
    print(f"FALHA: {len(problems)} problema(s)")
    for p in problems:
        print("  " + p)
    print("VEREDITO: FALHA")
    sys.exit(1)
print("VEREDITO: OK")
