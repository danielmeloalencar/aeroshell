#!/usr/bin/env python3
"""02m: ZERO configures da decoração entre press e release de um resize grab.

Especificação do humano (2026-11-20): o `send_frame_configure(..., "grab-start")` no
início do grab força um resize do GTK/WebKit; por alguns frames o buffer grande
chega com layout antigo e o 9-slice recorta pixels errados. Durante o grab o
buffer deve ficar congelado no último tamanho assentado; só o `source=final` no
unset/release é permitido (shell repinta e re-mede as slices).

Medição: todas as linhas `FRAME_CONFIGURE` são impressas cruas, com o tempo
relativo ao press, sem descartar amostra. VM sem 3D: só geometria/sequência.
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

SOCK = f"wayland-aero-gh-{os.getpid()}"
LOG = f"/tmp/gh-comp-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/gh-input-{os.getpid()}.sock"
STEPS = 8
STEP_PX = 8
MOTION_DELAY = 0.05

for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)
time.sleep(1)

# A instrumentação FRAME_CONFIGURE só existe com a feature bench.
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
press_ns = time.time_ns()
time.sleep(0.1)
for i in range(STEPS):
    s.sendall(f"motion {edge_x + (i + 1) * STEP_PX} {edge_y + (i + 1) * STEP_PX}\n".encode())
    time.sleep(MOTION_DELAY)
s.sendall(b"button release\n")
release_ns = time.time_ns()
time.sleep(1.0)

pat = re.compile(
    r"FRAME_CONFIGURE t=(\d+) serial=(\d+) window_id=(\d+) w=(\d+) h=(\d+) source=(\S+)"
)
configs = []
for l in log().splitlines():
    m = pat.search(l)
    if m:
        configs.append({
            "t": int(m.group(1)), "serial": int(m.group(2)), "wid": int(m.group(3)),
            "w": int(m.group(4)), "h": int(m.group(5)), "source": m.group(6), "raw": l,
        })

print(f"\n--- FRAME_CONFIGURE cru (relativo ao press; PRESS=0, RELEASE={(release_ns-press_ns)/1e6:.1f}ms) ---")
for c in configs:
    dt = (c["t"] - press_ns) / 1e6
    print(f"  t={dt:+9.2f}ms source={c['source']:12} wid={c['wid']} w={c['w']} h={c['h']} serial={c['serial']}")

during = [c for c in configs if press_ns <= c["t"] < release_ns]
final_after = [c for c in configs if c["t"] >= release_ns and c["source"] == "final"]

print(f"\nconfigures_no_grab={len(during)} source_final_apos_release={len(final_after)}")
problems = []
if during:
    for c in during:
        problems.append(f"configure DURANTE o grab: source={c['source']} w={c['w']} h={c['h']}")
if not final_after:
    problems.append("nenhum source=final após o release")

print()
if problems:
    print(f"FALHA: {len(problems)} problema(s)")
    for p in problems:
        print("  " + p)
    print("VEREDITO: FALHA")
    sys.exit(1)
print("VEREDITO: OK")
