#!/usr/bin/env python3
"""02o: medir PIXELS reais do framebuffer no soltar de um resize com arrasto.

Este teste NAO julga se o flicker existe: ele verifica se a medição capturou
evidência crua o suficiente para responder (a) se 9-slice e nativo diferem em
pixels no mesmo target e (b) se ha um transiente que coincide com o relato
humano. O teste 02n media apenas estados; aqui ha readback de pixels.

Metricas PRE-REGISTRADAS no cartao 02o (nao ajustar depois de ver os dados):
  - dump = grade 64x40 de medias de pixels ARGB8888 lidos do framebuffer;
  - diff = media de |canal RGB| por celula entre dois dumps;
  - flash candidato = frames consecutivos com dt <= 100 ms e >=10% das celulas
    mudando >=32/255;
  - par 9slice/nativo = ultimo frame rescaled=true com buf==target e o primeiro
    rescaled=false com buf==target.

Nenhuma amostra e descartada: todas as linhas PIXEL_FRAME e RESIZE_SLICE cruas
sao impressas; linhas corrompidas por interleaving sao contadas a parte.
VM sem 3D: sem conclusao de desempenho, apenas funcionamento e pixels relativos.
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

SOCK = f"wayland-aero-pxp-{os.getpid()}"
LOG = f"/tmp/pxp-comp-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/pxp-input-{os.getpid()}.sock"
DUMP_DIR = f"/tmp/opencode/02o_pixels_{os.getpid()}"
os.makedirs(DUMP_DIR, exist_ok=True)

STEPS = 8
STEP_PX = 8
MOTION_DELAY = 0.05
GRID_W = 64
GRID_H = 40

for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)
time.sleep(1)

# A instrumentacao PIXEL_FRAME so existe com a feature bench.
subprocess.run(
    ["cargo", "build", "-p", "aeroshell-compositor", "--features", "bench"],
    cwd=ROOT, check=True, stdout=subprocess.DEVNULL,
)
subprocess.run(
    ["cargo", "build", "-p", "aeroshell-shell", "--example", "frame_webview"],
    cwd=ROOT, check=True, stdout=subprocess.DEVNULL,
)

# frame_webview usa application_id fixo (frame_webview.rs:470). Sem um barramento
# proprio, o 2o shell ativa a instancia 02n aberta e morre antes do associate.
bus = subprocess.Popen(
    ["dbus-daemon", "--session", "--nofork", "--print-address=1"],
    stdout=subprocess.PIPE, text=True,
)
bus_address = bus.stdout.readline().strip()

env = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(RUST_LOG="debug", AERO_TEST_INPUT_SOCK=INPUT_SOCK,
           AERO_PIXEL_DUMP_DIR=DUMP_DIR, DBUS_SESSION_BUS_ADDRESS=bus_address)
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
    if bus.poll() is None:
        bus.terminate()
        try:
            bus.wait(timeout=5)
        except subprocess.TimeoutExpired:
            bus.kill()


atexit.register(cleanup)


def log():
    return re.sub(r"\x1b\[[0-9;]*m", "", open(LOG, errors="ignore").read())


def wait_for(pattern, timeout_s=20.0, label="condicao"):
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
print(f"dump_dir={DUMP_DIR} grade={GRID_W}x{GRID_H} "
      f"(inclui regiao A da barra e regiao B do canto)", flush=True)

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

# --- leitura crua, sem descartar amostras -----------------------------------
pixel_pat = re.compile(
    r"PIXEL_FRAME t=(\d+) frame=(\d+) size=(\d+)x(\d+) gen=(\d+) "
    r"release=(true|false) sha=([0-9a-f]+)"
)
pixels = []
for line in log().splitlines():
    m = pixel_pat.search(line)
    if m:
        pixels.append({
            "t": int(m.group(1)), "n": int(m.group(2)),
            "size": (int(m.group(3)), int(m.group(4))),
            "gen": int(m.group(5)), "release": m.group(6) == "true",
            "sha": m.group(7), "raw": line,
        })

slice_pat = re.compile(
    r"RESIZE_SLICE t=(\d+) window_id=(\d+) buf=(\d+)x(\d+) target=(\d+)x(\d+) "
    r"rescaled=(true|false)"
)
slices = []
for line in log().splitlines():
    m = slice_pat.search(line)
    if m and int(m.group(2)) == 0:
        slices.append({
            "t": int(m.group(1)),
            "buf": (int(m.group(3)), int(m.group(4))),
            "target": (int(m.group(5)), int(m.group(6))),
            "rescaled": m.group(7) == "true",
        })

print(f"\n--- PIXEL_FRAME (cruo; {len(pixels)} linhas) ---")
for p in pixels:
    print("  " + p["raw"])

final_ts = [int(m.group(1)) for m in re.finditer(
    r"FRAME_CONFIGURE t=(\d+) \S+ window_id=0 \S+ \S+ source=final", log(),
)]
final_t = final_ts[-1] if final_ts else release_ns
post = [x for x in slices if x["t"] >= final_t]
print(f"\n--- RESIZE_SLICE apos source=final (cruo; {len(post)} linhas) ---")
for x in post:
    print(f"  t={(x['t'] - final_t) / 1e6:+9.2f}ms buf={x['buf'][0]}x{x['buf'][1]} "
          f"target={x['target'][0]}x{x['target'][1]} rescaled={x['rescaled']}")

# Cada PIXEL_FRAME herda o estado RESIZE_SLICE mais recente (mesmo render).
def state_for(t):
    cand = [x for x in slices if x["t"] <= t]
    return cand[-1] if cand else None

states = [state_for(p["t"]) for p in pixels]


def read_dump(n):
    path = os.path.join(DUMP_DIR, f"frame_{n:06d}.bin")
    if not os.path.exists(path):
        return None
    data = open(path, "rb").read()
    if len(data) != GRID_W * GRID_H * 3:
        return None
    return data


def diff(a, b):
    if a is None or b is None:
        return None, None
    total_diff = 0
    changed = 0
    for x, y in zip(a, b):
        d = abs(x - y)
        total_diff += d
        if d >= 32:
            changed += 1
    return total_diff / len(a), changed / len(a)


# Par 9-slice/nativo: MESMO TARGET DE APRESENTACAO, transicao
# rescaled=true -> false. O buffer pode (e normalmente vai) estar menor que o
# target no frame 9-slice: e exatamente o que o 9-slice estica. Exigir
# buf==target era um bug de deteccao (o readback mais lento removeu o frame
# intermediario). Os limiares de diff/flash NAO mudaram.
pairs = []
for i in range(len(pixels) - 1):
    a, b = states[i], states[i + 1]
    if a is None or b is None:
        continue
    if a["rescaled"] and not b["rescaled"] and a["target"] == b["target"]:
        d, changed = diff(read_dump(pixels[i]["n"]), read_dump(pixels[i + 1]["n"]))
        pairs.append((i, d, changed))

# Flashes candidatos (pre-registrado): dt <=100 ms, >=10% das celulas >=32/255.
flashes = []
for i in range(len(pixels) - 1):
    dt = (pixels[i + 1]["t"] - pixels[i]["t"]) / 1e6
    d, changed = diff(read_dump(pixels[i]["n"]), read_dump(pixels[i + 1]["n"]))
    if d is not None and 0 < dt <= 100 and changed is not None and changed >= 0.10:
        flashes.append((i, dt, d, changed))

all_diffs = []
for i in range(len(pixels) - 1):
    d, _ = diff(read_dump(pixels[i]["n"]), read_dump(pixels[i + 1]["n"]))
    if d is not None:
        all_diffs.append(d)

print(f"\nframes_capturados={len(pixels)} dumps_uteis={len(all_diffs) + 1 if pixels else 0} "
      f"pares_9slice_nativo={len(pairs)} flashes_candidatos={len(flashes)}")
for i, d, changed in pairs:
    a, b = states[i], states[i + 1]
    print(f"  PAR frame={pixels[i]['n']}->{pixels[i+1]['n']} "
          f"buf={a['buf'][0]}x{a['buf'][1]}->{b['buf'][0]}x{b['buf'][1]} "
          f"target={a['target'][0]}x{a['target'][1]} "
          f"diff_medio={d:.2f} celulas_>=32={changed*100:.1f}%")
for i, dt, d, changed in flashes:
    print(f"  FLASH frame={pixels[i]['n']}->{pixels[i+1]['n']} dt={dt:.1f}ms "
          f"diff_medio={d:.2f} celulas_>=32={changed*100:.1f}%")
if all_diffs:
    print(f"diff_medio_max={max(all_diffs):.2f} diff_medio_geral="
          f"{sum(all_diffs)/len(all_diffs):.2f}")

problems = []
if not pixels:
    problems.append("nenhum PIXEL_FRAME: readback nao instrumentado ou nao executou")
if not pairs:
    problems.append("nenhum par 9-slice/nativo capturado no mesmo target")
if not all_diffs:
    problems.append("nenhum dump .bin util para comparar pixels")

print()
if problems:
    print(f"VEREDITO_MEDICAO: NAO_CAPTUROU" if not pixels else
          ("VEREDITO_MEDICAO: PAR_NAO_CAPTURADO" if not pairs else "VEREDITO_MEDICAO: NAO_CAPTUROU"))
    for p in problems:
        print("  " + p)
    sys.exit(1)
print("VEREDITO_MEDICAO: CAPTUROU")
