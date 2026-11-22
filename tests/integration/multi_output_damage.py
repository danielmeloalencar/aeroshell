#!/usr/bin/env python3
"""F1c: todos os outputs winit renderizam com damage incremental.

O baseline só renderiza ``aero-0`` com ``age=0``: este teste falha primeiro ao
esperar ``OUTPUT_FRAME output=aero-1``. A implementação deve emitir, sob
``bench``, uma linha por output e tentativa de redraw::

  OUTPUT_FRAME output=aero-<n> seq=<n> age=<n> damage=<rects|none>

Depois do primeiro ``foot`` estabilizar, os dois outputs precisam apresentar
``damage=none`` com ``age>0``. Um segundo ``foot`` é aberto então: pelo menos
um retângulo de dano do output que o recebe deve ser menor que o output inteiro.

VM sem 3D: funcionamento/damage apenas, não desempenho. O log completo é
preservado; trechos crus e linhas malformadas são impressos.
"""
import atexit
import glob
import os
import re
import socket
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/debug/aeroshell-compositor")
FRAME = os.path.join(ROOT, "target/debug/examples/frame_webview")
OUT_DIR = "/tmp/opencode/f1c"
os.makedirs(OUT_DIR, exist_ok=True)

SOCK = f"wayland-aero-multi-{os.getpid()}"
LOG = os.path.join(OUT_DIR, f"multi-output-{os.getpid()}.log")
INPUT_SOCK = os.path.join(OUT_DIR, f"multi-input-{os.getpid()}.sock")
PIXELS = os.path.join(OUT_DIR, f"multi-pixels-{os.getpid()}")
os.makedirs(PIXELS, exist_ok=True)
OUTPUT_SIZE = (640, 480)
CLEAR_RGB = (20, 20, 25)
OUTPUTS = ("aero-0", "aero-1")
FRAME_RE = re.compile(
    r"OUTPUT_FRAME output=(\S+) seq=(\d+) age=(\d+) damage=(none|[0-9x@,;-]+)"
)

for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)

subprocess.run(
    ["cargo", "build", "-p", "aeroshell-compositor", "--features", "bench"],
    cwd=ROOT,
    check=True,
    stdout=subprocess.DEVNULL,
)
subprocess.run(
    ["cargo", "build", "-p", "aeroshell-shell", "--example", "frame_webview"],
    cwd=ROOT,
    check=True,
    stdout=subprocess.DEVNULL,
)

env = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(RUST_LOG="info", AERO_TEST_INPUT_SOCK=INPUT_SOCK, AERO_PIXEL_DUMP_DIR=PIXELS)
log_file = open(LOG, "w")
comp = subprocess.Popen(
    [
        COMP,
        "--backend",
        "winit",
        "--socket",
        SOCK,
        "--size",
        f"{OUTPUT_SIZE[0]}x{OUTPUT_SIZE[1]}",
        "--outputs",
        "2",
        "--shell",
        FRAME,
        "--shell-env",
        "LD_PRELOAD=libgtk4-layer-shell.so.0",
        "--command",
        "foot -w 400x300",
    ],
    env=env,
    stdout=log_file,
    stderr=subprocess.STDOUT,
)
second_foot = None


def cleanup():
    """Encerra somente os PIDs iniciados por este harness."""
    global second_foot
    for process in (second_foot, comp):
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
    log_file.close()


atexit.register(cleanup)


def log():
    return re.sub(r"\x1b\[[0-9;]*m", "", open(LOG, errors="ignore").read())


def frames():
    parsed = []
    malformed = []
    for line in log().splitlines():
        if "OUTPUT_FRAME " not in line:
            continue
        match = FRAME_RE.search(line)
        if not match:
            malformed.append(line)
            continue
        output, seq, age, damage = match.groups()
        parsed.append(
            {
                "seq": int(seq),
                "output": output,
                "age": int(age),
                "damage": damage,
                "raw": line,
            }
        )
    return parsed, malformed


def wait_until(predicate, timeout_s, label):
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        result = predicate()
        if result:
            return result
        if comp.poll() is not None:
            print(f"FALHA: compositor terminou ({comp.returncode}) esperando {label}", file=sys.stderr)
            return False
        time.sleep(0.1)
    print(f"FALHA: timeout esperando {label}", file=sys.stderr)
    return False


def has_real_frame(window_id):
    pattern = (
        rf"RESIZE_GLUE t=\d+ window_id={window_id} content=\d+x\d+ "
        r"target=\d+x\d+ buffer=\d+x\d+ rescaled=(?:true|false)"
    )
    return re.search(pattern, log()) is not None


def slice_geometries(window_id, slice_index):
    pattern = (
        rf"SLICE_GEOM t=\d+ window_id={window_id} i={slice_index} "
        r"x=(-?\d+) y=(-?\d+) w=(\d+) h=(\d+)"
    )
    return re.findall(pattern, log())


def stable_frames():
    parsed, _ = frames()
    stable = {}
    for output in OUTPUTS:
        lines = [frame for frame in parsed if frame["output"] == output]
        if len(lines) >= 3 and all(
            frame["age"] > 0 and frame["damage"] == "none" for frame in lines[-3:]
        ):
            stable[output] = lines[-3:]
    return stable if len(stable) == len(OUTPUTS) else None


def damage_rects(value):
    if value == "none":
        return []
    result = []
    for rect in value.split(";"):
        match = re.fullmatch(r"(\d+)x(\d+)@(-?\d+),(-?\d+)", rect)
        if match:
            result.append(tuple(int(number) for number in match.groups()))
    return result


def partial_damage_after(seq, output="aero-0"):
    for frame in frames()[0]:
        if frame["seq"] <= seq or frame["output"] != output or frame["age"] == 0:
            continue
        for width, height, x, y in damage_rects(frame["damage"]):
            if (
                0 < width <= OUTPUT_SIZE[0]
                and 0 < height <= OUTPUT_SIZE[1]
                and width * height < OUTPUT_SIZE[0] * OUTPUT_SIZE[1]
            ):
                return frame, (width, height, x, y)
    return None


# Teste-primeiro: no baseline esta é a primeira condição que falha.
if not wait_until(
    lambda: "OUTPUT_FRAME output=aero-1 " in log(),
    12.0,
    "OUTPUT_FRAME output=aero-1 (render do segundo output)",
):
    print(log()[-4000:], file=sys.stderr)
    sys.exit(1)
if not wait_until(
    lambda: has_real_frame(0),
    20.0,
    "buffer real do frame da janela 0",
):
    print(log()[-4000:], file=sys.stderr)
    sys.exit(1)
stable = wait_until(stable_frames, 20.0, "3 frames estáveis por output com age>0 e damage=none")
if not stable:
    print("\n".join(frame["raw"] for frame in frames()[0][-30:]), file=sys.stderr)
    sys.exit(1)
print("--- frames estáveis (cruos) ---")
for output in OUTPUTS:
    for frame in stable[output]:
        print(frame["raw"])

# Uma mudança real de cena DEPOIS do estado estável deve produzir dano parcial.
change_seq = max(frame["seq"] for frame in frames()[0])
second_env = os.environ.copy()
second_env.pop("WAYLAND_SOCKET", None)
second_env["WAYLAND_DISPLAY"] = SOCK
second_foot = subprocess.Popen(
    ["foot", "-w", "320x240"],
    env=second_env,
    stdout=open(LOG, "a"),
    stderr=subprocess.STDOUT,
)
if not wait_until(
    lambda: has_real_frame(1) and slice_geometries(1, 1),
    20.0,
    "segundo foot com frame real e geometria válida",
):
    print(log()[-4000:], file=sys.stderr)
    sys.exit(1)
if not wait_until(lambda: partial_damage_after(change_seq), 15.0, "damage parcial após o segundo foot"):
    print("\n".join(frame["raw"] for frame in frames()[0][-40:]), file=sys.stderr)
    sys.exit(1)

partial, rect = partial_damage_after(change_seq)
print("--- mudança com damage parcial (crua) ---")
print(partial["raw"])
print(f"retangulo_parcial={rect} output={OUTPUT_SIZE}")

# Leva o segundo foot ao output 1. Isso prova conteúdo e coordenadas locais,
# não apenas duas linhas de log para duas texturas potencialmente vazias.
if not wait_until(lambda: os.path.exists(INPUT_SOCK), 5.0, "socket de input sintético"):
    sys.exit(1)
geometries = slice_geometries(1, 1)
if not geometries:
    print("FALHA: geometria da titlebar do segundo foot ausente", file=sys.stderr)
    sys.exit(1)
x, y, width, height = (int(value) for value in geometries[-1])
start = (x + width // 2, y + height // 2)
move_seq = max(frame["seq"] for frame in frames()[0])
input_socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
input_socket.connect(INPUT_SOCK)


def send_input(line, delay=0.05):
    input_socket.sendall((line + "\n").encode())
    time.sleep(delay)


send_input(f"motion {start[0]} {start[1]}")
send_input("button press", 0.1)
for step in range(1, 9):
    send_input(f"motion {start[0] + step * 80} {start[1]}")
send_input("button release", 0.1)
input_socket.close()
if not wait_until(
    lambda: partial_damage_after(move_seq, "aero-1"),
    12.0,
    "conteúdo do segundo foot com damage parcial em aero-1",
):
    print("\n".join(frame["raw"] for frame in frames()[0][-50:]), file=sys.stderr)
    sys.exit(1)
second_partial, second_rect = partial_damage_after(move_seq, "aero-1")
print("--- segundo output com conteúdo (crua) ---")
print(second_partial["raw"])
print(f"retangulo_aero_1={second_rect}")

parsed, malformed = frames()
print("--- primeiras linhas OUTPUT_FRAME (cruas) ---")
for frame in parsed[:8]:
    print(frame["raw"])
print(f"linhas_OUTPUT_FRAME={len(parsed)} malformadas={len(malformed)}")
if malformed:
    print("--- malformadas (preservadas, não usadas como evidência) ---")
    print("\n".join(malformed[:10]))

# Juiz visual F1c (medição objetiva, complementar ao humano): o canvas inicial
# do frame é >= output + insets (ADR 0007, 652x516 aqui). Se a layer surface do
# frame cair no fallback de "sem cliente mapeado" (winit.rs:184-202), ela é
# desenhada INTEIRA na origem (0,0) de cada output — uma faixa de ~640px (32
# colunas do grid) atravessa a metade da tela. Um frame do tamanho do foot
# (408x335) ocupa ~20 colunas. Medir TODOS os dumps; nenhuma amostra é descartada.
def decoração_por_metade():
    pior = {}
    paths = sorted(glob.glob(os.path.join(PIXELS, "frame_*.bin")))
    for path in paths:
        data = open(path, "rb").read()
        if len(data) != 64 * 40 * 3:
            continue
        for half, (x0, x1) in enumerate(((0, 32), (32, 64))):
            colunas = set()
            for y in range(40):
                for x in range(x0, x1):
                    offset = (y * 64 + x) * 3
                    r, g, b = data[offset], data[offset + 1], data[offset + 2]
                    # cinza neutro da decoração (titlebar/borda), 36 e 43 vistos
                    if r == g == b and 35 <= r <= 60:
                        colunas.add(x)
            pior[half] = max(pior.get(half, 0), len(colunas))
    return pior, len(paths)


# O host apresenta o framebuffer com a convenção Flipped180. Com a composição
# correta, a titlebar lógica (x,y) deve estar em (x,host_h-y-h) no readback.
# A dupla inversão vista pelo humano deixa essa célula no clear color.
orientation_error = None
pixel_match = re.search(r"PIXEL_FRAME t=\d+ frame=(\d+) size=(\d+)x(\d+)", log())
regions = slice_geometries(0, 0)
if not pixel_match:
    orientation_error = "nenhuma captura PIXEL_FRAME do host com frame real"
elif not regions:
    orientation_error = "geometria da titlebar da janela 0 ausente para orientação"
else:
    frame_number, host_w, host_h = (int(value) for value in pixel_match.groups())
    x, y, region_w, region_h = (int(value) for value in regions[-1])
    path = os.path.join(PIXELS, f"frame_{frame_number:06}.bin")
    if not os.path.exists(path):
        orientation_error = f"dump do host ausente: {path}"
    else:
        data = open(path, "rb").read()
        if len(data) != 64 * 40 * 3:
            orientation_error = f"dump do host com {len(data)} bytes"
        else:
            buffer_y = host_h - y - region_h
            center_x = x + region_w // 2
            center_y = buffer_y + region_h // 2
            cx = min(63, center_x * 64 // host_w)
            cy = min(39, center_y * 40 // host_h)
            offset = (cy * 64 + cx) * 3
            rgb = tuple(data[offset:offset + 3])
            difference = sum(abs(rgb[index] - CLEAR_RGB[index]) for index in range(3)) / 3
            print(f"host_orientacao frame={frame_number} regiao={regions[-1]} "
                  f"celula=({cx},{cy}) rgb={rgb} diff_clear={difference:.2f}")
            if difference < 10:
                orientation_error = (
                    f"titlebar não está na região Flipped180 esperada; rgb={rgb} "
                    f"~= clear={CLEAR_RGB} (possível dupla inversão)"
                )

problems = []
extent, dump_count = decoração_por_metade()
print(f"decoracao_por_metade_colunas={extent} dumps={dump_count} limite=28/32")
for half, colunas in extent.items():
    if colunas >= 28:
        problems.append(
            f"output {half}: decoração ocupa {colunas}/32 colunas da metade "
            "(frame do outro output desenhado inteiro no fallback)"
        )
if orientation_error:
    problems.append(orientation_error)
for output in OUTPUTS:
    lines = [frame for frame in parsed if frame["output"] == output]
    if not lines:
        problems.append(f"sem OUTPUT_FRAME para {output}")
    elif not any("damage=" in frame["raw"] for frame in lines):
        problems.append(f"sem damage= para {output}")
if not stable:
    problems.append("frames estáveis sem age>0/damage=none")
if rect[0] * rect[1] >= OUTPUT_SIZE[0] * OUTPUT_SIZE[1]:
    problems.append("mudança do cliente continuou com damage de output inteiro")
if problems:
    for problem in problems:
        print("FALHA: " + problem)
    print("VEREDITO: FALHA")
    sys.exit(1)
print(f"log_completo={LOG}")
print("VEREDITO: OK")
