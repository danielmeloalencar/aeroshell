#!/usr/bin/env python3
"""02p: pixel-diff FULL RES da titlebar durante o release de um resize.

O teste nasce para FALHAR no comportamento anterior ao canvas fixo. Cada dump
contém todos os pixels BGRA da titlebar apresentada, sem grade/agregação. Para
frames consecutivos de MESMO target após o release, qualquer pixel RGB diferente
é um transiente. Também são falha: voltar ao caminho nativo ou mudar o canvas no
release. Nenhuma amostra é descartada; os arquivos crus ficam em /tmp/opencode.

VM sem 3D: isto mede funcionamento/pixels, não desempenho.
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
SOCK = f"wayland-aero-02p-pixels-{os.getpid()}"
LOG = f"/tmp/opencode/02p-titlebar-comp-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/opencode/02p-titlebar-input-{os.getpid()}.sock"
GROWTH = len(sys.argv) > 1 and sys.argv[1] == "growth"
if len(sys.argv) > 1 and not GROWTH:
    print("Uso: frame_fixed_canvas_release_pixels.py [growth]", file=sys.stderr)
    sys.exit(2)
DUMP_DIR = f"/tmp/opencode/02p_titlebar_{'growth_' if GROWTH else ''}{os.getpid()}"
STEPS = 8
STEP_PX = 80 if GROWTH else 8
MOTION_DELAY = 0.05

os.makedirs(DUMP_DIR, exist_ok=True)
for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)
time.sleep(1)

subprocess.run(
    ["cargo", "build", "-p", "aeroshell-compositor", "--features", "bench"],
    cwd=ROOT, check=True,
)
subprocess.run(
    ["cargo", "build", "-p", "aeroshell-shell", "--example", "frame_webview"],
    cwd=ROOT, check=True,
)

# O application_id do shell é fixo; um barramento privado impede ativação de
# outra instância que o humano possa ter deixado aberta.
bus = subprocess.Popen(
    ["dbus-daemon", "--session", "--nofork", "--print-address=1"],
    stdout=subprocess.PIPE, text=True,
)
bus_address = bus.stdout.readline().strip()
env = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(
    RUST_LOG="debug",
    AERO_TEST_INPUT_SOCK=INPUT_SOCK,
    AERO_TITLEBAR_DUMP_DIR=DUMP_DIR,
    DBUS_SESSION_BUS_ADDRESS=bus_address,
)
comp_log = open(LOG, "w")
comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK, "--command", "foot",
     "--shell", FRAME, "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
    env=env, stdout=comp_log, stderr=subprocess.STDOUT,
)


def cleanup():
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()
    comp_log.close()
    if bus.poll() is None:
        bus.terminate()
        try:
            bus.wait(timeout=5)
        except subprocess.TimeoutExpired:
            bus.kill()


atexit.register(cleanup)


def log():
    return re.sub(r"\x1b\[[0-9;]*m", "", open(LOG, errors="ignore").read())


def wait_for(pattern, timeout_s=30.0, label="condição"):
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
if not wait_for(r"TITLEBAR_FRAME .*rescaled=", label="primeira captura full-res"):
    sys.exit(1)

m = re.findall(
    r"frame configure \(conteúdo do cliente\).*?content_w=(\d+).*?content_h=(\d+)"
    r".*?total_w=(\d+).*?total_h=(\d+)", log(),
)
if not m:
    print("FALHA: configure do frame não chegou", file=sys.stderr)
    sys.exit(1)
config = tuple(int(v) for v in m[-1])
INSETS = (6, 30, 6, 6)
frame_origin = (60 - INSETS[0], 80 - INSETS[1])
extras = (config[2] - config[0], config[3] - config[1])
total = (config[0] + extras[0], config[1] + extras[1])
edge_x = frame_origin[0] + total[0] - 3
edge_y = frame_origin[1] + total[1] - 3
print(f"dump_dir={DUMP_DIR} modo={'growth' if GROWTH else 'release'}")
print(f"conteudo={config[:2]} total={total} canto=({edge_x},{edge_y})", flush=True)
time.sleep(2.0)

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(INPUT_SOCK)
s.sendall(f"motion {edge_x} {edge_y}\n".encode())
time.sleep(0.2)
# Corrida pré-existente do press: só inicia o gesto depois que o compositor
# registra o grab. Todas as tentativas ficam impressas; nenhuma é descartada.
grab_started = False
for attempt in range(1, 4):
    mark = len(log())
    s.sendall(b"button press\n")
    deadline = time.time() + 2.0
    while time.time() < deadline:
        if re.search(r"resize do frame: grab iniciado", log()[mark:]):
            grab_started = True
            break
        time.sleep(0.02)
    print(f"tentativa_press={attempt} grab={grab_started}", flush=True)
    if grab_started:
        break
    s.sendall(b"button release\n")
    time.sleep(0.1)
    s.sendall(f"motion {edge_x} {edge_y}\n".encode())
    time.sleep(0.2)
if not grab_started:
    print("FALHA: nenhum grab após 3 presses", file=sys.stderr)
    sys.exit(1)
for i in range(STEPS):
    s.sendall(f"motion {edge_x + (i + 1) * STEP_PX} {edge_y + (i + 1) * STEP_PX}\n".encode())
    time.sleep(MOTION_DELAY)
s.sendall(b"button release\n")
release_ns = time.time_ns()
# Espera o gesto completo e o target próximo do final, não um sleep cego: o
# readback pode tornar a fila mais lenta nesta VM.
deadline = time.time() + 15.0
while time.time() < deadline:
    text = log()
    motions = len(re.findall(r"RESIZE_SYNC_MOTION t=", text))
    targets = [
        int(match.group(1))
        for match in re.finditer(r"TITLEBAR_FRAME .*?target=(\d+)x\d+", text)
    ]
    if motions >= STEPS and any(width >= total[0] + 48 for width in targets):
        break
    time.sleep(0.1)
time.sleep(1.5)
s.close()

pattern = re.compile(
    r"TITLEBAR_FRAME t=(\d+) frame=(\d+) window_id=(\d+) x=(-?\d+) y=(-?\d+) "
    r"w=(\d+) h=(\d+) buf=(\d+)x(\d+) target=(\d+)x(\d+) "
    r"rescaled=(true|false) release=(true|false) sha=([0-9a-f]+)"
)
frames = []
malformed = []
for line in log().splitlines():
    if "TITLEBAR_FRAME" not in line:
        continue
    match = pattern.search(line)
    if not match:
        malformed.append(line)
        continue
    g = match.groups()
    if int(g[2]) != 0:
        continue
    frames.append({
        "t": int(g[0]), "n": int(g[1]),
        "rect": tuple(int(v) for v in g[3:7]),
        "buf": (int(g[7]), int(g[8])),
        "target": (int(g[9]), int(g[10])),
        "rescaled": g[11] == "true", "release": g[12] == "true",
        "raw": line,
    })

print(f"\n--- TITLEBAR_FRAME cru ({len(frames)} linhas; malformadas={len(malformed)}) ---")
for frame in frames:
    print(frame["raw"])
for line in malformed:
    print("MALFORMADA: " + line)


def read_frame(frame):
    path = os.path.join(DUMP_DIR, f"titlebar_{frame['n']:06}.bgra")
    expected = frame["rect"][2] * frame["rect"][3] * 4
    data = open(path, "rb").read()
    if len(data) != expected:
        raise ValueError(f"{path}: {len(data)} bytes, esperado {expected}")
    return data


def rgb_diff(before, after):
    changed = 0
    total = 0
    for i in range(0, len(before), 4):
        delta = sum(abs(before[i + channel] - after[i + channel]) for channel in range(3))
        total += delta
        if delta:
            changed += 1
    pixels = len(before) // 4
    return changed, total / (pixels * 3)


# Sanidade da região/orientação: a cor sólida da barra (#2b2b2b) precisa dominar
# ao menos uma captura. Sem isso, um retângulo errado não pode virar evidência.
sanity = 0
for frame in frames:
    data = read_frame(frame)
    bar = sum(
        1 for i in range(0, len(data), 4)
        if data[i] == 43 and data[i + 1] == 43 and data[i + 2] == 43
    )
    if bar * 2 > len(data) // 4:
        sanity += 1

post = [frame for frame in frames if frame["t"] >= release_ns]
pairs = []
canvas_changes = 0
for before, after in zip(post, post[1:]):
    if before["buf"] != after["buf"]:
        canvas_changes += 1
    if before["target"] != after["target"] or before["rect"] != after["rect"]:
        continue
    changed, mean = rgb_diff(read_frame(before), read_frame(after))
    pairs.append((before, after, changed, mean))

transients = [pair for pair in pairs if pair[2] != 0]
changed_pixels = sum(pair[2] for pair in pairs)
diff_max = max((pair[3] for pair in pairs), default=0.0)
native_frames = sum(1 for frame in post if not frame["rescaled"])
motion_count = len(re.findall(r"RESIZE_SYNC_MOTION t=", log()))
max_target_w = max((frame["target"][0] for frame in frames), default=0)
print(f"\nframes_pos_release={len(post)} pares_mesmo_target={len(pairs)} "
      f"sanity_titlebar={sanity} motions={motion_count} max_target_w={max_target_w}")
growth_completed = len(re.findall(r"FRAME_PAINTED .*growth_completed=true", log()))
print(f"changed_pixels={changed_pixels} transientes={len(transients)} "
      f"diff_medio_max={diff_max:.4f} native_frames={native_frames} "
      f"canvas_changes={canvas_changes} growth_completed={growth_completed}")
for before, after, changed, mean in transients:
    print(
        f"TRANSIENTE frames={before['n']}->{after['n']} "
        f"buf={before['buf']}->{after['buf']} target={after['target']} "
        f"rescaled={before['rescaled']}->{after['rescaled']} "
        f"changed_pixels={changed} diff_medio={mean:.4f}"
    )

problems = []
if malformed:
    problems.append(f"{len(malformed)} linhas TITLEBAR_FRAME malformadas")
if not post:
    problems.append("nenhuma captura após o release")
if not sanity:
    problems.append("região não contém a cor dominante da titlebar; captura inválida")
if not pairs:
    problems.append("nenhum par full-res com mesmo target após o release")
if motion_count < STEPS:
    problems.append(f"gesto incompleto: {motion_count}/{STEPS} motions no grab")
if max_target_w < total[0] + 48:
    problems.append(f"target não alcançou o final do gesto: max={max_target_w}")
if changed_pixels:
    problems.append(f"{changed_pixels} pixels mudaram com target constante")
if native_frames:
    problems.append(f"{native_frames} frames abandonaram o 9-slice")
if GROWTH:
    if canvas_changes < 1:
        problems.append("target > canvas não produziu crescimento de canvas")
    if growth_completed < 1:
        problems.append("crescimento não foi confirmado por frame_painted")
elif canvas_changes:
    problems.append(f"canvas/buffer mudou {canvas_changes} vez(es) no release")

if problems:
    print("VEREDITO: FALHA")
    for problem in problems:
        print("  " + problem)
    sys.exit(1)
print("VEREDITO: OK")
