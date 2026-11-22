#!/usr/bin/env python3
"""F1b/F1c: render headless real, damage incremental e pixels do 9-slice.

O compositor roda com EGL surfaceless e um ``foot`` real. Sob ``bench`` exige:
  - HEADLESS_FRAME output=<nome> age=<n> damage=<rects|none>;
  - RENDER_SHARED window_id=0 kind=frame (não apenas placeholder);
  - três frames estáveis com age>0 e damage=none;
  - damage parcial depois que um segundo foot muda a cena;
  - PIXEL_FRAME e dump 64*40*3 cuja célula da titlebar, localizada por
    SLICE_GEOM e convertida por Flipped180, difere do clear color.

A fórmula de Flipped180 é a de smithay-0.7.0/src/utils/geometry.rs:1684-1710:
(x, y, w, h) -> (x, output_h-y-h, w, h). VM sem 3D: prova de funcionamento,
não desempenho. Logs e dumps completos são preservados.
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
LOG = f"/tmp/opencode/f1c/headless-{os.getpid()}.log"
PIXELS = f"/tmp/opencode/f1c/headless-pixels-{os.getpid()}"
os.makedirs("/tmp/opencode/f1c", exist_ok=True)
os.makedirs(PIXELS, exist_ok=True)
OUTPUT_SIZE = (640, 480)
CLEAR_RGB = (20, 20, 25)  # [0.08, 0.08, 0.10] * 255
HEADLESS_RE = re.compile(
    r"HEADLESS_FRAME output=(\S+)(?: age=(\d+))? damage=(none|[0-9x@,;-]+)"
)

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
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(RUST_LOG="info", AERO_PIXEL_DUMP_DIR=PIXELS)
log_file = open(LOG, "w")
comp = subprocess.Popen(
    [COMP, "--backend", "headless", "--socket", SOCK,
     "--size", f"{OUTPUT_SIZE[0]}x{OUTPUT_SIZE[1]}",
     "--shell", FRAME, "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0",
     "--command", "foot -w 400x300"],
    env=env, stdout=log_file, stderr=subprocess.STDOUT,
)
second_foot = None


@atexit.register
def cleanup():
    """Encerra apenas os PIDs iniciados por este harness."""
    for process in (second_foot, comp):
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
    log_file.close()


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


def headless_frames():
    rows = []
    malformed = []
    for line in log().splitlines():
        if "HEADLESS_FRAME output=" not in line:
            continue
        match = HEADLESS_RE.search(line)
        if not match:
            malformed.append(line)
            continue
        output, age, damage = match.groups()
        rows.append({
            "output": output,
            "age": int(age) if age is not None else None,
            "damage": damage,
            "raw": line,
        })
    return rows, malformed


def stable_rows():
    rows, _ = headless_frames()
    if len(rows) >= 3 and all(
        row["age"] is not None and row["age"] > 0 and row["damage"] == "none"
        for row in rows[-3:]
    ):
        return rows[-3:]
    return None


def damage_rects(value):
    if value == "none":
        return []
    result = []
    for rect in value.split(";"):
        match = re.fullmatch(r"(\d+)x(\d+)@(-?\d+),(-?\d+)", rect)
        if match:
            result.append(tuple(int(number) for number in match.groups()))
    return result


def partial_row(start):
    for row in headless_frames()[0][start:]:
        if row["age"] is None or row["age"] == 0:
            continue
        for width, height, x, y in damage_rects(row["damage"]):
            if (0 < width <= OUTPUT_SIZE[0] and 0 < height <= OUTPUT_SIZE[1]
                    and width * height < OUTPUT_SIZE[0] * OUTPUT_SIZE[1]):
                return row, (width, height, x, y)
    return None


problems = []
if not wait_for(r"HEADLESS_FRAME output=", label="HEADLESS_FRAME"):
    sys.exit(1)
if not wait_for(r"RENDER_SHARED window_id=0 kind=frame", label="RENDER_SHARED kind=frame"):
    sys.exit(1)
if not wait_for(r"PIXEL_FRAME t=", label="PIXEL_FRAME"):
    sys.exit(1)

# O baseline F1b sempre usa age=0/full damage: esta espera falha sem impedir
# que as demais medições (inclusive pixels) sejam impressas ao final.
stable_deadline = time.time() + 12.0
while time.time() < stable_deadline and not stable_rows():
    time.sleep(0.1)
stable = stable_rows()
if not stable:
    problems.append("sem 3 HEADLESS_FRAME estáveis consecutivos com age>0 e damage=none")
else:
    print("--- HEADLESS_FRAME estáveis (cruas) ---")
    for row in stable:
        print("  " + row["raw"])

change_start = len(headless_frames()[0])
second_env = os.environ.copy()
second_env.pop("WAYLAND_SOCKET", None)
second_env["WAYLAND_DISPLAY"] = SOCK
second_foot = subprocess.Popen(
    ["foot", "-w", "320x240"], env=second_env,
    stdout=open(LOG, "a"), stderr=subprocess.STDOUT,
)
partial_deadline = time.time() + 15.0
while time.time() < partial_deadline and not partial_row(change_start):
    time.sleep(0.1)
partial = partial_row(change_start)
if not partial:
    problems.append("segundo foot não produziu retângulo de damage menor que o output")
else:
    print("--- HEADLESS_FRAME com dano parcial (crua) ---")
    print("  " + partial[0]["raw"])
    print(f"retangulo_parcial={partial[1]} output={OUTPUT_SIZE}")

text = log()
frames = [l for l in text.splitlines() if "HEADLESS_FRAME output=" in l]
shared = [l for l in text.splitlines() if "RENDER_SHARED window_id=" in l]
pixels = [l for l in text.splitlines() if "PIXEL_FRAME t=" in l]
_, malformed = headless_frames()

print("--- HEADLESS_FRAME (primeiras 5, cruas) ---")
for line in frames[:5]:
    print("  " + line)
print("--- RENDER_SHARED (cruas) ---")
for line in shared[:5]:
    print("  " + line)
print("--- PIXEL_FRAME (cruas) ---")
for line in pixels[:5]:
    print("  " + line)
print(f"HEADLESS_FRAME malformadas={len(malformed)}")
if malformed:
    print("\n".join(malformed[:10]))

if len(frames) < 3:
    problems.append(f"apenas {len(frames)} HEADLESS_FRAME (mínimo 3)")
if not any("damage=" in line for line in frames):
    problems.append("nenhuma HEADLESS_FRAME com campo damage=")
if not any(re.search(r"window_id=0 kind=frame\b", line) for line in shared):
    problems.append("RENDER_SHARED window_id=0 kind=frame ausente")
if not pixels:
    problems.append("nenhuma PIXEL_FRAME")

# Associa cada dump à geometria real do topo do frame no MESMO ciclo de render.
pixel_records = []
current_kind = None
top_region = None
for line in text.splitlines():
    shared_match = re.search(r"RENDER_SHARED window_id=0 kind=(frame|placeholder)", line)
    if shared_match:
        current_kind = shared_match.group(1)
    geom_match = re.search(
        r"SLICE_GEOM t=\d+ window_id=0 i=0 x=(-?\d+) y=(-?\d+) w=(\d+) h=(\d+)",
        line,
    )
    if geom_match:
        top_region = tuple(int(value) for value in geom_match.groups())
    pixel_match = re.search(r"PIXEL_FRAME t=\d+ frame=(\d+) size=(\d+)x(\d+)", line)
    if pixel_match and top_region is not None:
        pixel_records.append({
            "frame": int(pixel_match.group(1)),
            "size": (int(pixel_match.group(2)), int(pixel_match.group(3))),
            "kind": current_kind,
            "region": top_region,
        })

region_measurements = []
for record in pixel_records:
    if record["kind"] != "frame":
        continue
    name = f"frame_{record['frame']:06}.bin"
    path = os.path.join(PIXELS, name)
    if not os.path.exists(path):
        continue
    data = open(path, "rb").read()
    if len(data) != 64 * 40 * 3:
        continue
    width, height = record["size"]
    x, y, region_w, region_h = record["region"]
    # Flipped180: x permanece; o retângulo vai para output_h-y-h.
    buffer_y = height - y - region_h
    center_x = x + region_w // 2
    center_y = buffer_y + region_h // 2
    if not (0 <= center_x < width and 0 <= center_y < height):
        continue
    cx = min(63, center_x * 64 // width)
    cy = min(39, center_y * 40 // height)
    offset = (cy * 64 + cx) * 3
    rgb = tuple(data[offset:offset + 3])
    difference = sum(abs(rgb[i] - CLEAR_RGB[i]) for i in range(3)) / 3
    region_measurements.append((record["frame"], (cx, cy), rgb, difference, record["region"]))

print(f"regioes_frame_medidas={len(region_measurements)}")
for measurement in region_measurements[:5]:
    print(f"  frame={measurement[0]} celula={measurement[1]} rgb={measurement[2]} "
          f"diff_clear={measurement[3]:.2f} regiao={measurement[4]}")
if not region_measurements:
    problems.append("nenhum dump kind=frame correlacionado à região SLICE_GEOM do topo")
elif not any(measurement[3] >= 10 for measurement in region_measurements):
    problems.append(f"região da titlebar igual ao clear color {CLEAR_RGB} em todos os dumps")

dumps = sorted(f for f in os.listdir(PIXELS) if re.fullmatch(r"frame_\d{6}\.bin", f))
nonzero = 0
for name in dumps:
    data = open(os.path.join(PIXELS, name), "rb").read()
    if len(data) != 64 * 40 * 3:
        problems.append(f"{name}: {len(data)} bytes (esperado {64*40*3})")
        continue
    if any(data):
        nonzero += 1
print(f"dumps={len(dumps)} com_conteudo_nao_nulo={nonzero} (dir={PIXELS})")
if not dumps:
    problems.append("nenhum dump frame_*.bin da sonda 02o")
elif nonzero == 0:
    problems.append("todos os dumps são 64x40x3 zerados")

print(f"HEADLESS_FRAME={len(frames)} RENDER_SHARED={len(shared)} PIXEL_FRAME={len(pixels)}")
if problems:
    for problem in problems:
        print("  FALHA: " + problem)
    print(f"log_completo={LOG}")
    print("VEREDITO: FALHA")
    sys.exit(1)
print(f"log_completo={LOG}")
print("VEREDITO: OK")
