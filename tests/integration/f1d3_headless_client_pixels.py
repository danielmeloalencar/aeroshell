#!/usr/bin/env python3
"""F1d3: medir pixels REAIS do conteúdo do cliente no backend headless.

O compositor headless usa o MESMO ``render/scene.rs`` do udev, mas com renderer
EGL surfaceless e readback via ``ExportMem`` (``frame_pixel_probe.rs``). Este
harness não julga a tela do tty3: ele separa "cena/import compartilhados" de
"apresentação GBM/DRM do udev" ao responder uma pergunta objetiva:

    A região de conteúdo do foot (60,80,696,494) contém pixels diferentes do
    clear (20,20,25) DEPOIS que o frame WebKit passa a existir?

A grade é 64x40 sobre o output inteiro (1920x955, o mesmo modo do log do tty3).
O output headless usa ``Transform::Flipped180`` (``headless.rs:64``), então a
região lógica (60,80,696,494) vira (60,381,696,494) no framebuffer — a mesma
conversão que ``headless_render_damage.py:249-250`` aplica à titlebar.
Nenhuma amostra é descartada: baseline e todas as amostras após o map do cliente
são impressas. Só os PIDs iniciados aqui são encerrados no ``atexit``.
"""
import atexit
import os
import re
import subprocess
import sys
import time
from statistics import median

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/debug/aeroshell-compositor")
FRAME = os.path.join(ROOT, "target/debug/examples/frame_webview")

SOCK = f"wayland-aero-f1d3-pixels-{os.getpid()}"
OUT = "/tmp/opencode/f1d3-pixels"
LOG = os.path.join(OUT, f"headless-{os.getpid()}.log")
PIXELS = os.path.join(OUT, f"dumps-{os.getpid()}")
SIZE = (1920, 955)
CLEAR = (20, 20, 25)
CONTENT = (60, 80, 696, 494)  # x, y, w, h — medido no log do tty3
GRID = (64, 40)
CELL_TOLERANCE = 8  # distância média por canal contra o clear

os.makedirs(OUT, exist_ok=True)
os.makedirs(PIXELS, exist_ok=True)
socket_path = f"/run/user/1000/{SOCK}"
if os.path.exists(socket_path):
    os.unlink(socket_path)

subprocess.run(
    ["cargo", "build", "-p", "aeroshell-compositor", "--features", "bench"],
    cwd=ROOT, check=True, stdout=subprocess.DEVNULL,
)
subprocess.run(
    ["cargo", "build", "-p", "aeroshell-shell", "--example", "frame_webview"],
    cwd=ROOT, check=True, stdout=subprocess.DEVNULL,
)

# O application_id do shell é fixo; um barramento privado impede que outra
# instância (por exemplo, a sessão udev que o humano deixou aberta) ative o
# novo frame_webview e o derrube antes do foot mapear.
bus = subprocess.Popen(
    ["dbus-daemon", "--session", "--nofork", "--print-address=1"],
    stdout=subprocess.PIPE, text=True,
)
bus_address = bus.stdout.readline().strip()

env = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(RUST_LOG="info", AERO_PIXEL_DUMP_DIR=PIXELS,
           DBUS_SESSION_BUS_ADDRESS=bus_address)
log_file = open(LOG, "w")
comp = subprocess.Popen(
    [COMP, "--backend", "headless", "--socket", SOCK,
     "--size", f"{SIZE[0]}x{SIZE[1]}",
     "--shell", FRAME, "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0",
     "--command", "foot -w 696x494"],
    env=env, stdout=log_file, stderr=subprocess.STDOUT,
)


@atexit.register
def cleanup():
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()
    log_file.close()
    if bus.poll() is None:
        bus.terminate()
        try:
            bus.wait(timeout=5)
        except subprocess.TimeoutExpired:
            bus.kill()


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


def content_cells():
    x, y, w, h = CONTENT
    # Flipped180: x permanece; y no framebuffer = output_h - y - h.
    y = SIZE[1] - y - h
    cells = []
    for cy in range(GRID[1]):
        y0 = cy * SIZE[1] // GRID[1]
        y1 = max(y0 + 1, (cy + 1) * SIZE[1] // GRID[1])
        if y1 <= y or y0 >= y + h:
            continue
        for cx in range(GRID[0]):
            x0 = cx * SIZE[0] // GRID[0]
            x1 = max(x0 + 1, (cx + 1) * SIZE[0] // GRID[0])
            if x1 <= x or x0 >= x + w:
                continue
            cells.append((cx, cy))
    return cells


CELLS = content_cells()


def stats(frame_number):
    path = os.path.join(PIXELS, f"frame_{frame_number:06}.bin")
    data = open(path, "rb").read()
    if len(data) != GRID[0] * GRID[1] * 3:
        return None
    colors = []
    for cx, cy in CELLS:
        offset = (cy * GRID[0] + cx) * 3
        colors.append(tuple(data[offset:offset + 3]))
    dist = [sum(abs(color[i] - CLEAR[i]) for i in range(3)) / 3 for color in colors]
    return {
        "frame": frame_number,
        "cells": len(colors),
        "non_clear": sum(value >= CELL_TOLERANCE for value in dist),
        "bright": sum(max(color) >= 80 for color in colors),
        "distinct": len(set(colors)),
        "mean_diff": round(sum(dist) / len(dist), 2),
        "max_diff": round(max(dist), 2),
        "sample_colors": sorted(set(colors))[:6],
    }


problems = []
if not wait_for(r"HEADLESS_FRAME output=", label="HEADLESS_FRAME"):
    sys.exit(1)
if not wait_for(r"toplevel mapeado no space", label="cliente foot mapeado"):
    sys.exit(1)
if not wait_for(r"RENDER_SHARED window_id=0 kind=frame", label="frame WebKit"):
    sys.exit(1)
if not wait_for(r"PIXEL_FRAME t=", label="PIXEL_FRAME"):
    sys.exit(1)

# Depois do primeiro frame WebKit, deixar o foot estabilizar e acumular amostras.
time.sleep(3.0)

text = log()
pixel_lines = [line for line in text.splitlines() if "PIXEL_FRAME t=" in line]
frame_numbers = []
for line in pixel_lines:
    match = re.search(r"PIXEL_FRAME t=\d+ frame=(\d+) size=(\d+)x(\d+)", line)
    if match and (int(match.group(2)), int(match.group(3))) == SIZE:
        frame_numbers.append(int(match.group(1)))

first_client_line = next(
    (i for i, line in enumerate(text.splitlines())
     if "toplevel mapeado no space" in line),
    len(text.splitlines()),
)
first_pixel_after_client = None
for line_number, line in enumerate(text.splitlines()):
    if line_number <= first_client_line:
        continue
    if "PIXEL_FRAME t=" in line:
        match = re.search(r"PIXEL_FRAME t=\d+ frame=(\d+)", line)
        if match:
            first_pixel_after_client = int(match.group(1))
            break

baseline = [stats(number) for number in frame_numbers
            if first_pixel_after_client is None or number < first_pixel_after_client]
baseline = [item for item in baseline if item is not None]
after = [stats(number) for number in frame_numbers
         if first_pixel_after_client is not None and number >= first_pixel_after_client]
after = [item for item in after if item is not None]

buffer_y = SIZE[1] - CONTENT[1] - CONTENT[3]
print(f"output={SIZE[0]}x{SIZE[1]} content_logico={CONTENT} "
      f"content_framebuffer={(CONTENT[0], buffer_y, CONTENT[2], CONTENT[3])} "
      f"celulas={len(CELLS)} clear={CLEAR}")
print(f"PIXEL_FRAME linhas={len(pixel_lines)} dumps_usaveis={len(baseline)+len(after)} "
      f"primeiro_apos_cliente={first_pixel_after_client}")
print("--- baseline (antes do frame WebKit, cru) ---")
for item in baseline:
    print(f"  frame={item['frame']} non_clear={item['non_clear']}/{item['cells']} "
          f"bright={item['bright']} distinct={item['distinct']} "
          f"mean_diff={item['mean_diff']} max_diff={item['max_diff']} "
          f"colors={item['sample_colors']}")
print("--- depois do map do cliente foot (cru) ---")
for item in after:
    print(f"  frame={item['frame']} non_clear={item['non_clear']}/{item['cells']} "
          f"bright={item['bright']} distinct={item['distinct']} "
          f"mean_diff={item['mean_diff']} max_diff={item['max_diff']} "
          f"colors={item['sample_colors']}")

if not baseline:
    problems.append("nenhum dump baseline válido")
if not after:
    problems.append("nenhum dump válido depois do map do cliente foot")
if baseline and after:
    baseline_max = max(item["non_clear"] for item in baseline)
    after_median = median(item["non_clear"] for item in after)
    threshold = len(CELLS) // 2
    print(f"baseline_non_clear_max={baseline_max} after_non_clear_median={after_median} "
          f"limite_conteudo={threshold}")
    if baseline_max > threshold // 4:
        problems.append(f"baseline já tem {baseline_max} células fora do clear")
    if after_median < threshold:
        problems.append("região de conteúdo permaneceu igual ao clear no headless")

print(f"log_completo={LOG} dumps={PIXELS}")
if problems:
    for problem in problems:
        print("  FALHA: " + problem)
    print("VEREDITO: FALHA — conteúdo do cliente NÃO apareceu no headless")
    sys.exit(1)
print("VEREDITO: OK — conteúdo do cliente apareceu no headless (cena compartilhada)")
