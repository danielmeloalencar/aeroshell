#!/usr/bin/env python3
"""02j: auditar os diagnósticos nomeados do resize da decoração.

Instrumentação esperada do compositor (`--features bench`):
  FRAME_NEVER_ACKED t=<ns> serial=<s> window_id=<id> w=<w> h=<h> target_w=<tw> target_h=<th>
    buffer_w=<bw> buffer_h=<bh> waited_ms=<ms> start_t=<ns> end_t=<ns> duration_ms=<ms> max_scale=<f>
  FRAME_NEVER_DREW_AT_SIZE  (mesmos campos)
  FRAME_STRETCH_ACTIVE      (mesmos campos; duration_ms > 0)

Uso:
  python3 tests/integration/frame_diagnostics.py normal
  python3 tests/integration/frame_diagnostics.py faults

`faults` usa injeção de falha SOMENTE bench para tornar determinísticos o configure
sem ack e o configure sem buffer no alvo. Nada disso existe sem a feature bench.

Regra do humano: a decoração aparece 1-2 s depois que a janela abre → espera 2,0 s
após o primeiro buffer antes de arrastar. VM sem 3D: nenhuma conclusão de desempenho.
Nenhuma amostra é descartada em silêncio: as linhas cruas são impressas.
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

PHASE = sys.argv[1] if len(sys.argv) > 1 else "normal"
if PHASE not in ("normal", "faults"):
    print("Uso: frame_diagnostics.py {normal|faults}", file=sys.stderr)
    sys.exit(2)

SOCK = f"wayland-aero-diag-{PHASE}-{os.getpid()}"
LOG = f"/tmp/diag-comp-{PHASE}-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/diag-input-{PHASE}-{os.getpid()}.sock"
# 02j: o arrasto de 14 passos/50 ms da 02f não faz o WebKit atrasar nesta VM
# (rescaled=true = 0 medido); 40 passos/10 ms forçam o episódio de escala real.
STEPS = 40
STEP_PX = 10
MOTION_DELAY = 0.01
DECORATION_APPEAR_DELAY_S = 2.0

NAMES = ("FRAME_NEVER_ACKED", "FRAME_NEVER_DREW_AT_SIZE", "FRAME_STRETCH_ACTIVE")
FIELDS = (
    "t", "serial", "window_id", "w", "h", "target_w", "target_h",
    "buffer_w", "buffer_h", "waited_ms", "start_t", "end_t", "duration_ms", "max_scale",
)
LINE_RE = re.compile(
    r"^(FRAME_NEVER_ACKED|FRAME_NEVER_DREW_AT_SIZE|FRAME_STRETCH_ACTIVE) "
    + " ".join(rf"{f}=(\S+)" for f in FIELDS)
    + r"$"
)

for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)
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
env.update(RUST_LOG="debug", AERO_TEST_INPUT_SOCK=INPUT_SOCK)
if PHASE == "faults":
    env.update(AERO_DIAG_SUPPRESS_ACK_ONCE="1", AERO_DIAG_SUPPRESS_DRAW_ONCE="1")

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

time.sleep(DECORATION_APPEAR_DELAY_S)
print(f"fase={PHASE} decoracao_assentada: aguardo extra de {DECORATION_APPEAR_DELAY_S:.1f}s antes do arrasto")

extras = (config[2] - config[0], config[3] - config[1])
INSETS = (6, 30, 6, 6)
content_origin = (60, 80)
frame_origin = (content_origin[0] - INSETS[0], content_origin[1] - INSETS[1])
total = (config[0] + extras[0], config[1] + extras[1])
edge_x = frame_origin[0] + total[0] - 3
mid_y = frame_origin[1] + total[1] // 2
print(f"conteudo_inicial={config[:2]} extras={extras} total={total} borda_direita=({edge_x},{mid_y})")

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(INPUT_SOCK)


def send(cmd, delay=MOTION_DELAY):
    s.sendall((cmd + "\n").encode())
    if delay:
        time.sleep(delay)


send(f"motion {edge_x} {mid_y}")
send("button press", 0.1)
for i in range(STEPS):
    send(f"motion {edge_x + (i + 1) * STEP_PX} {mid_y}")
send("button release", 0)
time.sleep(1.2)

text = log()
candidates = [l for l in text.splitlines() if any(n + " " in l for n in NAMES)]
parsed = {}
malformed = []
for line in candidates:
    m = LINE_RE.match(line)
    if not m:
        malformed.append(line)
        continue
    name = m.group(1)
    parsed.setdefault(name, []).append(
        {field: m.group(i + 2) for i, field in enumerate(FIELDS)}
    )

print(f"linhas: candidatas={len(candidates)} malformadas={len(malformed)}")
for name in NAMES:
    print(f"  {name}: {len(parsed.get(name, []))}")
for line in candidates:
    print(f"CRU: {line}")
for line in malformed:
    print(f"MALFORMADA: {line}")

problems = []
if malformed:
    problems.append(f"{len(malformed)} linha(s) de diagnóstico malformada(s)")

for name in ("FRAME_NEVER_ACKED", "FRAME_NEVER_DREW_AT_SIZE"):
    seen = set()
    for ev in parsed.get(name, []):
        serial = ev["serial"]
        if serial in seen:
            problems.append(f"{name}: serial {serial} duplicado")
        seen.add(serial)
        try:
            waited = int(ev["waited_ms"])
            minimum = 100 if name == "FRAME_NEVER_ACKED" else 300
            if waited < minimum:
                problems.append(f"{name}: waited_ms={waited} < {minimum}")
            for field in ("w", "h", "target_w", "target_h"):
                if int(ev[field]) <= 0:
                    problems.append(f"{name}: {field}={ev[field]} não positivo")
            # buffer só é evidência do diagnóstico de desenho; no de ack ele
            # pode legitimamente ser desconhecido (0).
            if name == "FRAME_NEVER_DREW_AT_SIZE":
                for field in ("buffer_w", "buffer_h"):
                    if int(ev[field]) <= 0:
                        problems.append(f"{name}: {field}={ev[field]} não positivo")
        except (ValueError, KeyError) as exc:
            problems.append(f"{name}: campo inválido ({exc})")

if PHASE == "normal":
    stretches = parsed.get("FRAME_STRETCH_ACTIVE", [])
    if not any(int(ev["duration_ms"]) > 0 for ev in stretches):
        problems.append("nenhum FRAME_STRETCH_ACTIVE com duration_ms>0 durante o arrasto")
elif PHASE == "faults":
    for name in ("FRAME_NEVER_ACKED", "FRAME_NEVER_DREW_AT_SIZE"):
        if not parsed.get(name):
            problems.append(f"injeção de falha não gerou {name}")

if problems:
    print("VEREDITO: FALHA", file=sys.stderr)
    for p in problems:
        print(f"FALHA: {p}", file=sys.stderr)
    sys.exit(1)

print("VEREDITO: OK")
