#!/usr/bin/env python3
"""02e: medir a colagem ESPACIAL da decoração ao conteúdo commitado.

O compositor compilado com `--features bench` registra, por render com frame
associado:
  RESIZE_GLUE t=<ns> window_id=<id> content=<w>x<h> target=<w>x<h>
              buffer=<w>x<h> rescaled=<bool>

Invariante: target == conteúdo COMMITADO + extras (insets + shadow) e
rescaled == (buffer != target). Após soltar o grab, o repaint do WebKit deve
alcançar o alvo em ≤500 ms (rescaled=false e buffer==target).

VM sem 3D: nenhuma conclusão de desempenho; apenas geometria. Nenhuma amostra
é descartada em silêncio — as linhas cruas são impressas.
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

SOCK = f"wayland-aero-glue-{os.getpid()}"
LOG = f"/tmp/glue-comp-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/glue-input-{os.getpid()}.sock"
STEPS = 14
STEP_PX = 10
MOTION_DELAY = 0.05
SETTLE_MS = 500
TOL = 1

for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)
time.sleep(1)

# A instrumentação só existe com a feature bench (AGENTS.md: variáveis/knobs de bench).
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

config = None
for _ in range(150):
    m = re.findall(
        r"frame configure \(conteúdo do cliente\).*?content_w=(\d+).*?content_h=(\d+)"
        r".*?total_w=(\d+).*?total_h=(\d+)",
        log(),
    )
    if m:
        config = tuple(int(v) for v in m[-1])
        break
    time.sleep(0.1)
if not config:
    print("FALHA: configure do frame não chegou", file=sys.stderr)
    print(log()[-2000:], file=sys.stderr)
    sys.exit(1)

# O WebView pode levar alguns segundos para produzir o PRIMEIRO buffer. Sem ele
# não há elemento para importar no render e a instrumentação não emite linhas.
# Esperar o pré-requisito evita falsa falha por infraestrutura (amostra crua de
# /tmp/glue-comp-58443.log: 28 FRAME_BUFFER_COMMIT, todos com buffer=None).
for _ in range(200):
    if re.search(r"FRAME_BUFFER_COMMIT buffer=Some\(\(\d+, \d+\)\)", log()):
        break
    time.sleep(0.1)
else:
    print("FALHA: frame WebView não commitou buffer inicial", file=sys.stderr)
    print(log()[-2000:], file=sys.stderr)
    sys.exit(1)

extras = (config[2] - config[0], config[3] - config[1])
INSETS = (6, 30, 6, 6)
content_origin = (60, 80)
frame_origin = (content_origin[0] - INSETS[0], content_origin[1] - INSETS[1])
total = (config[0] + extras[0], config[1] + extras[1])
edge_x = frame_origin[0] + total[0] - 3
mid_y = frame_origin[1] + total[1] // 2
print(f"conteudo_inicial={config[:2]} extras={extras} total={total} borda_direita=({edge_x},{mid_y})")
print(f"parametros: passos={STEPS} delta={STEP_PX}px delay={MOTION_DELAY:.3f}s settle={SETTLE_MS}ms")

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
release_ns = time.time_ns()
time.sleep(1.2)

text = log()
lines = [l for l in text.splitlines() if "RESIZE_GLUE " in l]
glue = []
malformed = []
for l in lines:
    m = re.search(
        r"RESIZE_GLUE t=(\d+) window_id=(\d+) content=(\d+)x(\d+) "
        r"target=(\d+)x(\d+) buffer=(\d+)x(\d+) rescaled=(true|false)", l,
    )
    if not m:
        # Interleaving de stdout entre compositor e filhos pode corromper uma
        # linha. Contabilizamos em vez de descartar em silêncio.
        malformed.append(l)
        continue
    if m:
        t, wid = int(m.group(1)), int(m.group(2))
        content = (int(m.group(3)), int(m.group(4)))
        target = (int(m.group(5)), int(m.group(6)))
        buffer = (int(m.group(7)), int(m.group(8)))
        glue.append((t, wid, content, target, buffer, m.group(9) == "true", l))

print(f"\nlinhas RESIZE_GLUE={len(glue)} malformadas={len(malformed)}")
if malformed:
    print("exemplos malformados: " + repr(malformed[:3]))
print("\n--- trecho cru das medições (primeiras e últimas 15 linhas) ---")
print("\n".join(lines[:15]))
print("...")
print("\n".join(lines[-15:]))

if not glue:
    print("FALHA: nenhuma linha RESIZE_GLUE (instrumentação ausente?)", file=sys.stderr)
    sys.exit(1)

problems = []
during = [g for g in glue if g[0] < release_ns]
if not during:
    problems.append("nenhuma linha RESIZE_GLUE durante o grab")

for t, wid, content, target, buffer, rescaled, raw in during:
    if min(content) <= 0 or min(buffer) <= 0:
        continue
    want = (content[0] + extras[0], content[1] + extras[1])
    if any(abs(a - b) > TOL for a, b in zip(target, want)):
        problems.append(f"target {target} != content+extras {want}")
    if rescaled != (buffer != target):
        problems.append(f"rescaled={rescaled} mas buffer={buffer} target={target}")

settled = [g for g in glue if release_ns <= g[0] <= release_ns + SETTLE_MS * 1_000_000]
ok_settle = [g for g in settled if not g[5] and g[4] == g[3] and min(g[2]) > 0]
if not settled:
    problems.append(f"nenhuma linha RESIZE_GLUE nos {SETTLE_MS} ms após soltar")
elif not ok_settle:
    problems.append(f"após soltar: nenhuma linha com buffer==target e rescaled=false")

print(f"durante_grab={len(during)} apos_soltar={len(settled)} colados={len(ok_settle)}")
if problems:
    print("FALHA: " + "; ".join(problems[:5]), file=sys.stderr)
    verdict = "FALHA"
else:
    verdict = "OK"
print(f"\nVEREDITO: {verdict}")
print("(esperado: target == content + extras e rescaled == (buffer != target) no grab; "
      "buffer==target e rescaled=false após soltar)")
sys.exit(0 if verdict == "OK" else 1)
