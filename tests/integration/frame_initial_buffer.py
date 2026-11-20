#!/usr/bin/env python3
"""02m: nenhum frame pode nascer com o buffer mínimo 200x200 do WebKit.

O defeito reproduzido no teste visual do humano (docs/STATE.md §6/§7): a 2a
janela era associada com `buf=200x200 target=708x530 rescaled=true
slice_scale_x=3.7021` por 7 frames (~5 ms) antes do primeiro buffer real.
O shell descarta o tamaho do `frame_requested` e cria a WebView sem tamanho;
o configure com o tamanho real chega depois (frame_webview.rs:215-229, 260).

Medição: todas as linhas RESIZE_SLICE da 2a janela são impressas cruas, sem
descartar amostra. VM sem 3D: só geometria, nenhuma conclusão de desempenho.
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

SOCK = f"wayland-aero-ibuf-{os.getpid()}"
LOG = f"/tmp/ibuf-comp-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/ibuf-input-{os.getpid()}.sock"
SCALE_LIMIT = 2.0  # 200x200 -> 708x530 dá sx=3.70; um frame real dá sx~1.

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
comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK, "--command", "foot",
     "--shell", FRAME, "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
    env=env, stdout=open(LOG, "w"), stderr=subprocess.STDOUT,
)
foot2 = None


def cleanup():
    global foot2
    if foot2 is not None and foot2.poll() is None:
        foot2.terminate()
        try:
            foot2.wait(timeout=5)
        except subprocess.TimeoutExpired:
            foot2.kill()
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


if not wait_for(r"FRAMEWEB: associate\(window_id=\d+\)", label="1o associate"):
    sys.exit(1)
if not wait_for(r"FRAME_BUFFER_COMMIT buffer=Some\(\(\d+, \d+\)\)", label="1o buffer"):
    sys.exit(1)

# A 2a janela é onde o defeito apareceu no juiz visual.
client_env = env.copy()
client_env["WAYLAND_DISPLAY"] = SOCK
foot2 = subprocess.Popen(
    ["foot"], env=client_env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
)
if not wait_for(r"FRAMEWEB: associate\(window_id=1\)", label="2o associate"):
    sys.exit(1)
time.sleep(1.5)  # cobre os ~5 ms do flash e o primeiro buffer real

text = log()
print("--- frame_requested (cru) ---")
for l in text.splitlines():
    if "FRAMEWEB: frame_requested" in l:
        print("  " + l)

pat = re.compile(
    r"RESIZE_SLICE t=(\d+) window_id=(\d+) buf=(\d+)x(\d+) target=(\d+)x(\d+) "
    r"rescaled=(true|false).*?slice_scale_x=([0-9.]+) slice_scale_y=([0-9.]+)"
)
rows = []
for l in text.splitlines():
    m = pat.search(l)
    if m:
        rows.append(m)
print(f"\nlinhas RESIZE_SLICE={len(rows)}")
print("--- amostras da 2a janela (window_id=1), cruas ---")
for m in rows:
    if m.group(2) != "1":
        continue
    print(f"  t={m.group(1)} buf={m.group(3)}x{m.group(4)} target={m.group(5)}x{m.group(6)} "
          f"rescaled={m.group(7)} sx={m.group(8)} sy={m.group(9)}")

second = [m for m in rows if m.group(2) == "1"]
problems = []
if not second:
    problems.append("nenhuma linha RESIZE_SLICE da 2a janela")
else:
    anomalous = [
        m for m in second
        if float(m.group(8)) > SCALE_LIMIT or (m.group(3) == "200" and m.group(4) == "200")
    ]
    for m in anomalous:
        problems.append(
            f"buffer inicial escalado: buf={m.group(3)}x{m.group(4)} sx={m.group(8)} "
            f"t={m.group(1)}"
        )

print()
if problems:
    print(f"FALHA: {len(problems)} amostra(s) com buffer mínimo escalado")
    for p in problems[:5]:
        print("  " + p)
    print("VEREDITO: FALHA")
    sys.exit(1)
print("VEREDITO: OK")
