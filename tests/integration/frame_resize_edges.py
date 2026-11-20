#!/usr/bin/env python3
"""Passo 1: redimensionar pelo mouse nas 8 bordas/cantos do frame.

Geometria determinística: o Space posiciona o toplevel n=0 em (60,80)
(protocols/xdg_shell.rs:52). O frame_webview declara insets (6,30,6,6) e sombra 0,
então o frame nasce em (54,50). Cada probe faz motion -> press -> release SEM motion
durante o grab: assim nenhum tamanho muda e as coordenadas dos 8 probes permanecem
válidas. No fim, um grab completo (press -> motion -> release) prova que o resize
aplica de fato o novo tamanho.

Que cada press registre detecção em TODAS as 8 posições também prova que o grab
anterior foi encerrado na release: se a release não encerrasse o grab, os eventos
seguintes iriam ao grab ativo e só o primeiro press por rodada registraria.
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

SOCK = f"wayland-aero-edges-{os.getpid()}"
LOG = f"/tmp/edges-comp-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/edges-input-{os.getpid()}.sock"

# Nunca matar por nome: isto pode encerrar a aplicação do usuário. Cada execução
# tem socket e PID próprios; o cleanup abaixo encerra só o compositor iniciado aqui.
for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)
time.sleep(1)

env = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(RUST_LOG="debug", AERO_TEST_INPUT_SOCK=INPUT_SOCK)

comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK,
     "--command", "foot",
     "--shell", FRAME,
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
    env=env,
    stdout=open(LOG, "w"), stderr=subprocess.STDOUT,
)

def cleanup():
    """Encerra apenas o PID que este harness iniciou, mesmo em sys.exit antecipado."""
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()

atexit.register(cleanup)

def log():
    return re.sub(r"\x1b\[[0-9;]*m", "", open(LOG, errors="ignore").read())

for _ in range(100):
    if os.path.exists(INPUT_SOCK):
        break
    time.sleep(0.05)
else:
    print("FALHA: socket de input sintético não apareceu", file=sys.stderr)
    sys.exit(1)

# Espera o frame associar e o configure do conteúdo real.
content = None
for _ in range(120):
    m = re.findall(r"frame configure \(conteúdo do cliente\).*?content_w=(\d+).*?content_h=(\d+)", log())
    if m:
        content = (int(m[-1][0]), int(m[-1][1]))
        break
    time.sleep(0.1)
if not content:
    print("FALHA: configure do frame não chegou", file=sys.stderr)
    print(log()[-2000:], file=sys.stderr)
    sys.exit(1)

# Insets do frame_webview: left=6, top=30(titlebar), right=6, bottom=6.
INSETS = (6, 30, 6, 6)
content_origin = (60, 80)
frame_origin = (content_origin[0] - INSETS[0], content_origin[1] - INSETS[1])
total = (content[0] + INSETS[0] + INSETS[2], content[1] + INSETS[1] + INSETS[3])
print(f"conteudo={content} frame_origin={frame_origin} total={total}")

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(INPUT_SOCK)

def send(line):
    s.sendall((line + "\n").encode())
    time.sleep(0.45)

def last(pattern, since, timeout_s=2.0):
    """Espera a linha aparecer, em vez de depender de um sleep fixo.

    Amostra preservada em /tmp/opencode/02j_neighbor_frame_resize_edges.log:
    o log do compositor tinha `borda/canto -> resize Right`, mas o harness
    reportou `detectado=None` (corrida entre `len(log())` e o processamento).
    """
    deadline = time.time() + timeout_s
    while True:
        new = [m for m in re.finditer(pattern, log()) if m.start() >= since]
        if new or time.time() >= deadline:
            return new
        time.sleep(0.05)

EDGE_POINTS = {
    "TopLeft":     (frame_origin[0] + 3,        frame_origin[1] + 3),
    "Top":         (frame_origin[0] + total[0] // 2, frame_origin[1] + 3),
    "TopRight":    (frame_origin[0] + total[0] - 3,  frame_origin[1] + 3),
    "Left":        (frame_origin[0] + 3,        frame_origin[1] + total[1] // 2),
    "Right":       (frame_origin[0] + total[0] - 3,  frame_origin[1] + total[1] // 2),
    "BottomLeft":  (frame_origin[0] + 3,        frame_origin[1] + total[1] - 3),
    "Bottom":      (frame_origin[0] + total[0] // 2, frame_origin[1] + total[1] - 3),
    "BottomRight": (frame_origin[0] + total[0] - 3,  frame_origin[1] + total[1] - 3),
}

def probe(x, y):
    """Retorna (surface_id, ponto_devolvido) do hit-test para este ponto."""
    mark = len(log())
    send(f"motion {x} {y}")
    m = re.findall(r"TESTE ponteiro motion x=[\d.]+ y=[\d.]+ under=Some\(\(ObjectId\(wl_surface@\d+\[\d+\], (\d+)\), Point<[^>]+> \{ x: (-?[\d.]+), y: (-?[\d.]+) \}\)", log()[mark:])
    return (m[-1] if m else None)

s_id_content = probe(content_origin[0] + content[0] // 2, content_origin[1] + content[1] // 2)
print(f"\nprobe no conteudo ({content_origin[0]+content[0]//2},{content_origin[1]+content[1]//2}): "
      f"surface={s_id_content[0]} ponto=({s_id_content[1]},{s_id_content[2]}) -> caminho do CLIENTE")

print("\n=== probes de borda/canto (motion -> press -> release; sem motion durante o grab) ===")
rows = []
for edge, (x, y) in EDGE_POINTS.items():
    hit = probe(x, y)
    # Mesma corrida de foco vista em frame_configure_hygiene.py: o press pode
    # chegar ao compositor sem foco e o WebView não registra o botão. Repetir
    # REGISTRA cada tentativa; nenhuma amostra é descartada em silêncio.
    tentativas = []
    det = []
    grab = []
    for tentativa in range(1, 4):
        mark_press = len(log())
        send("button press")
        det = last(r"borda/canto -> resize (\w+) serial=\d+", mark_press)
        tentativas.append(f"{tentativa}={'ok' if det else 'sem deteccao'}")
        if det:
            break
        send("button release")
        time.sleep(0.1)
        send(f"motion {x} {y}")
    print(f"  {edge:11} tentativas de press: {', '.join(tentativas)}")
    send("button release")
    grab = last(r"resize do frame: grab iniciado.*?edges=(\w+)", mark_press)
    rows.append((edge, det[-1].group(1) if det else None, grab[-1].group(1) if grab else None,
                 hit, (x, y)))

ok = True
for edge, detected, grab_edge, hit, point in rows:
    on_frame = hit is not None and float(hit[1]) == float(frame_origin[0]) and float(hit[2]) == float(frame_origin[1])
    good = detected == edge and grab_edge == edge and on_frame and hit[0] != s_id_content[0]
    ok = ok and good
    print(f"{edge:11} ponto={str(point):12} hit_no_frame={on_frame} detectado={detected} grab={grab_edge} {'OK' if good else 'FALHA'}")

# Grab completo: prova que o resize aplica tamanho novo.
print("\n=== grab completo BottomRight (press -> motion -> release) ===")
before = re.findall(r"frame configure \(conteúdo do cliente\).*?content_w=(\d+).*?content_h=(\d+)", log())[-1]
mark = len(log())
x, y = EDGE_POINTS["BottomRight"]
send(f"motion {x} {y}")
send("button press")
send(f"motion {x + 40} {y + 30}")
send("button release")
after = re.findall(r"frame configure \(conteúdo do cliente\).*?content_w=(\d+).*?content_h=(\d+)", log())[-1]
print(f"conteudo antes={(int(before[0]), int(before[1]))} depois={(int(after[0]), int(after[1]))}")
grew = int(after[0]) > int(before[0]) and int(after[1]) > int(before[1])
print("resize aplicado:", "OK" if grew else "FALHA")

s.close()
cleanup()

verdict = ok and grew
print(f"\nVEREDITO: {'OK' if verdict else 'FALHA'}")
sys.exit(0 if verdict else 1)
