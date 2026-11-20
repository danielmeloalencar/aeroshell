#!/usr/bin/env python3
"""02f: medir a higiene dos configures da decoração durante um resize.

Instrumentação esperada do compositor (`--features bench`):
  RESIZE_SYNC_MOTION t=<ns> window_id=<id> want=<w>x<h>
  RESIZE_SYNC_CLIENT_CONFIGURE t=<ns> window_id=<id> w=<w> h=<h>
  FRAME_CONFIGURE t=<ns> serial=<s> window_id=<id> w=<w> h=<h> source=<grab|commit|probe|final|layer>
  RESIZE_SYNC_CLIENT t=<ns> size=<w>x<h>
  FRAME_BUFFER_COMMIT buffer=<..> expected=<..> ok=<bool> t=<ns>
  RESIZE_GLUE t=<ns> ... buffer=<w>x<h> rescaled=<bool>

Critério: configures de frame por motion <= 1/3; configure do cliente em TODOS
os motions; nenhum configure `source=commit` durante o grab; um `source=final`
após soltar com o tamanho REAL do conteúdo; buffer nativo em <=500 ms.

VM sem 3D: nenhuma conclusão de desempenho. Nenhuma amostra é descartada em
silêncio — as linhas cruas usadas no veredito são impressas.
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

SOCK = f"wayland-aero-hygiene-{os.getpid()}"
LOG = f"/tmp/hygiene-comp-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/hygiene-input-{os.getpid()}.sock"
STEPS = 14
STEP_PX = 10
MOTION_DELAY = 0.05
SETTLE_MS = 500
# Relato do humano: a decoração aparece 1-2 s depois que a janela abre.
# Esperar evita iniciar o arrasto antes de o frame WebView estar pronto.
DECORATION_APPEAR_DELAY_S = 2.0

for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)
time.sleep(1)

# A instrumentação só existe com a feature bench (AGENTS.md: knobs de bench).
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
    sys.exit(1)

for _ in range(200):
    if re.search(r"FRAME_BUFFER_COMMIT .*buffer=Some\(\(\d+, \d+\)\)", log()):
        break
    time.sleep(0.1)
else:
    print("FALHA: frame WebView não commitou buffer inicial", file=sys.stderr)
    sys.exit(1)

# A decoração pode demorar 1-2 s para aparecer mesmo depois do primeiro buffer.
time.sleep(DECORATION_APPEAR_DELAY_S)
print(f"decoracao_assentada: aguardo extra de {DECORATION_APPEAR_DELAY_S:.1f}s antes do arrasto")

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
t_press = time.time_ns()
send("button press", 0.1)
for i in range(STEPS):
    send(f"motion {edge_x + (i + 1) * STEP_PX} {mid_y}")
# O instante de soltura é o do sendall, NÃO depois do sleep do send(): o
# configure final do compositor acontece nessa janela de 100 ms.
send("button release", 0)
t_release = time.time_ns()
time.sleep(1.2)

text = log()
motions = [int(m) for m in re.findall(r"RESIZE_SYNC_MOTION t=(\d+)", text)]
client_configs = [
    (int(m.group(1)), m.group(2))
    for m in re.finditer(r"RESIZE_SYNC_CLIENT_CONFIGURE t=(\d+) .*?w=(\d+) h=(\d+)", text)
]
frame_configs = []
malformed = []
for line in text.splitlines():
    if "FRAME_CONFIGURE " not in line:
        continue
    m = re.search(
        r"FRAME_CONFIGURE t=(\d+) serial=(\d+) window_id=(\d+) w=(\d+) h=(\d+) source=(\w+)",
        line,
    )
    if m:
        frame_configs.append((
            int(m.group(1)), int(m.group(2)), int(m.group(3)),
            (int(m.group(4)), int(m.group(5))), m.group(6), line,
        ))
    else:
        malformed.append(line)
client_commits = [
    (int(m.group(1)), (int(m.group(2)), int(m.group(3))))
    for m in re.finditer(r"RESIZE_SYNC_CLIENT t=(\d+) size=(\d+)x(\d+)", text)
]
buffer_commits = [
    (int(m.group(2)), m.group(1) == "true", line)
    for m, line in (
        (re.search(r"FRAME_BUFFER_COMMIT .*?ok=(true|false) t=(\d+)", l), l)
        for l in text.splitlines()
    )
    if m
]
glue = []
for m in re.finditer(
    r"RESIZE_GLUE t=(\d+) window_id=(\d+) content=(\d+)x(\d+) "
    r"target=(\d+)x(\d+) buffer=(\d+)x(\d+) rescaled=(true|false)", text,
):
    glue.append((
        int(m.group(1)), (int(m.group(5)), int(m.group(6))),
        (int(m.group(7)), int(m.group(8))), m.group(9) == "true",
    ))

print(f"\nlinhas: motions={len(motions)} client_configs={len(client_configs)} "
      f"frame_configs={len(frame_configs)} malformadas={len(malformed)} "
      f"client_commits={len(client_commits)} buffer_commits={len(buffer_commits)} glue={len(glue)}")
if malformed:
    print("exemplos FRAME_CONFIGURE malformados: " + repr(malformed[:3]))

during_motions = [t for t in motions if t_press <= t <= t_release]
during_configs = [c for c in frame_configs if t_press <= c[0] <= t_release]
during_client = [c for c in client_configs if t_press <= c[0] <= t_release]
finals = [
    c for c in frame_configs
    if t_release <= c[0] <= t_release + 100_000_000 and c[4] == "final"
]
last_commit = [c for c in client_commits if c[0] <= t_release]
last_size = last_commit[-1][1] if last_commit else None
settled_glue = [
    g for g in glue
    if t_release <= g[0] <= t_release + SETTLE_MS * 1_000_000
    and not g[3] and g[1] == g[2] and min(g[1]) > 0
]
settled_buffers = [
    b for b in buffer_commits
    if t_release <= b[0] <= t_release + SETTLE_MS * 1_000_000 and b[1]
]

print("\n--- trecho cru das medições (primeiras e últimas 8 linhas de cada fonte) ---")
for label, lines in (
    ("RESIZE_SYNC_MOTION", [l for l in text.splitlines() if "RESIZE_SYNC_MOTION " in l]),
    ("RESIZE_SYNC_CLIENT_CONFIGURE", [l for l in text.splitlines() if "RESIZE_SYNC_CLIENT_CONFIGURE " in l]),
    ("FRAME_CONFIGURE", [l for l in text.splitlines() if "FRAME_CONFIGURE " in l]),
    ("FRAME_BUFFER_COMMIT", [l for l in text.splitlines() if "FRAME_BUFFER_COMMIT " in l]),
):
    print(f"[{label}]")
    print("\n".join(lines[:8]))
    if len(lines) > 8:
        print("...")
        print("\n".join(lines[-8:]))

problems = []
ratio = len(during_configs) / len(during_motions) if during_motions else float("inf")
print(f"\ndurante: motions={len(during_motions)} frame_configures={len(during_configs)} "
      f"client_configures={len(during_client)} ratio={ratio:.3f}")
print("por source (durante): " + repr({
    src: sum(1 for c in during_configs if c[4] == src)
    for src in sorted({c[4] for c in during_configs})
}))
print(f"apos soltar: finals={len(finals)} ultimo_cliente={last_size} "
      f"glue_colados={len(settled_glue)} buffers_ok={len(settled_buffers)}")

if not during_motions:
    problems.append("nenhuma linha RESIZE_SYNC_MOTION no intervalo do arrasto")
if not during_configs:
    problems.append("nenhuma linha FRAME_CONFIGURE (instrumentação ausente?)")
elif ratio > 1.0 / 3.0:
    problems.append(f"configures/motions={ratio:.3f} > 1/3")
if len(during_client) != len(during_motions):
    problems.append(f"configure do cliente {len(during_client)} != motions {len(during_motions)}")
commits_during = sum(1 for c in during_configs if c[4] == "commit")
if commits_during:
    problems.append(f"{commits_during} configure(s) source=commit durante o grab")
if len(finals) != 1:
    problems.append(f"final configs={len(finals)} (esperado 1)")
elif last_size is not None and finals[0][3] != last_size:
    problems.append(f"final {finals[0][3]} != tamanho real do cliente {last_size}")
if not settled_glue:
    problems.append(f"nenhum RESIZE_GLUE colado em {SETTLE_MS} ms após soltar")
if not settled_buffers:
    problems.append(f"nenhum FRAME_BUFFER_COMMIT ok=true em {SETTLE_MS} ms após soltar")

verdict = "FALHA" if problems else "OK"
if problems:
    print("FALHA: " + "; ".join(problems[:6]), file=sys.stderr)
print(f"\nVEREDITO: {verdict}")
sys.exit(0 if verdict == "OK" else 1)
