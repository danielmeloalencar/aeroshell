#!/usr/bin/env python3
"""02b: o frame segue e age SOMENTE na janela dona do clique (2 janelas).

Medição objetiva (não depende de olhar a tela):
  1. duas janelas mapeiam e o frame_host associa um frame por window_id;
  2. clique no conteúdo EXCLUSIVO da janela A (faixa x=60..100, fora de B) eleva A
     no Space (`raise_element`): A passa a ser o ÚLTIMO elemento do `list`;
  3. resize pela borda ESQUERDA do frame de A (x=57, fora de B) muda a geometria
     de A e NÃO muda a de B.

A borda esquerda é usada de propósito: x≈57 está fora da janela B (que começa
em x=100), então o hit-test não pode confundir os dois frames. O eixo y também
separa: A nasce em y=80, B em y=120, e resize na borda esquerda não muda y.

Por que isto falha antes da correção: window_id é um id lógico estável, mas o
compositor resolve `space.elements().nth(window_id)` — ordem z de renderização.
Depois do passo 2 a ordem inverte (space = [B, A]); `nth(0)` passa a devolver B,
e o resize do frame de A(redimensiona) B.
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

SOCK = f"wayland-aero-follow-{os.getpid()}"
LOG = f"/tmp/follow-comp-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/follow-input-{os.getpid()}.sock"
IPC_SOCK = f"/tmp/follow-ipc-{os.getpid()}.sock"
INSETS = (6, 30, 6, 6)
Y_A, Y_B = 80, 120  # new_toplevel: (60+n*40, 80+n*40)

for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK, IPC_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)

env = os.environ.copy()
for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(RUST_LOG="debug", AERO_TEST_INPUT_SOCK=INPUT_SOCK, AERO_WINDOW_IPC_SOCK=IPC_SOCK)

comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK, "--size", "900x600",
     "--command", "foot & foot", "--shell", FRAME,
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
    env=env, stdout=open(LOG, "w"), stderr=subprocess.STDOUT,
)


def cleanup():
    """Encerra apenas o PID que este harness iniciou."""
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()


atexit.register(cleanup)


def log():
    return re.sub(r"\x1b\[[0-9;]*m", "", open(LOG, errors="ignore").read())


def hits(pattern, since=None):
    return [m for m in re.finditer(pattern, log()) if since is None or m.start() >= since]


def wait(pattern, timeout=60, group=0):
    for _ in range(int(timeout * 10)):
        found = hits(pattern)
        if found:
            return found[-1].group(group)
        time.sleep(0.1)
    return None


def ipc(command):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(IPC_SOCK)
    s.sendall((command + "\n").encode())
    time.sleep(0.4)
    lines = [l for l in s.recv(4096).decode().splitlines() if l and l != "."]
    s.close()
    return lines


def rows():
    return [tuple(int(v) for v in line.split()[1:]) for line in ipc("list")]


def by_y(y):
    # `list` devolve "W id x y w h" -> tupla (id, x, y, w, h); y é o índice 2.
    for r in rows():
        if r[2] == y:
            return r
    return None


def content_of(window_id):
    m = hits(
        r"frame configure \(conteúdo do cliente\).*?window_id=(\d+).*?content_w=(\d+).*?content_h=(\d+)"
    )
    m = [x for x in m if int(x.group(1)) == window_id]
    return None if not m else (int(m[-1].group(2)), int(m[-1].group(3)))


if not wait("injeção de ponteiro sintético ativa", 15):
    print("FALHA: socket de input sintético não apareceu", file=sys.stderr)
    sys.exit(1)

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect(INPUT_SOCK)


def send(line):
    sock.sendall((line + "\n").encode())
    time.sleep(0.5)


def surface_at(x, y):
    """id do wl_surface sob (x,y), segundo o log de hit-test do compositor."""
    mark = len(log())
    send(f"motion {x} {y}")
    m = hits(
        r"TESTE ponteiro motion x=[\d.]+ y=[\d.]+ under=Some\(\(ObjectId\((wl_surface@\d+\[\d+\]), \d+\)",
        mark,
    )
    return None if not m else m[-1].group(1)


def focus_surfaces(since=None):
    """wl_surfaces que receberam foco de teclado (focus=true) desde `since`."""
    return [
        m.group(1)
        for m in hits(
            r"foco de teclado alterado surface=Some\(ObjectId\((wl_surface@\d+\[\d+\])"
            r"[^\n]*?focus=true",
            since,
        )
    ]


# 1) duas janelas + um frame por window_id
for _ in range(int(60 * 10)):
    if len(rows()) >= 2:
        break
    time.sleep(0.5)
else:
    print("FALHA: só uma janela apareceu no list", file=sys.stderr)
    sys.exit(1)

for _ in range(int(30 * 10)):
    assoc = sorted(set(int(m.group(1)) for m in hits(r"FRAMEWEB: associate\(window_id=(\d+)\)")))
    if len(assoc) >= 2:
        break
    time.sleep(0.2)

initial = rows()
print(f"janelas iniciais (ordem do space): {initial}")
print(f"associate por window_id: {assoc}")
if len(initial) < 2 or len(assoc) < 2:
    print("FALHA: pré-condição (2 janelas / 2 frames) não satisfeita", file=sys.stderr)
    sys.exit(1)

a0, b0 = by_y(Y_A), by_y(Y_B)
print(f"A(y={Y_A})={a0}  B(y={Y_B})={b0}")
if not a0 or not b0:
    print(f"FALHA: origens esperadas y={Y_A}/{Y_B} não encontradas", file=sys.stderr)
    sys.exit(1)

surf_a = surface_at(70, 100)   # faixa x=60..100: só A
surf_b = surface_at(150, 200)  # B começa em (100,120)
print(f"surface de A={surf_a}  surface de B={surf_b}")
if not surf_a or not surf_b or surf_a == surf_b:
    print(f"FALHA: não consegui separar as superfícies (A={surf_a}, B={surf_b})", file=sys.stderr)
    sys.exit(1)

# 2) clique no conteúdo EXCLUSIVO de A => raise_element(A) + foco em A
mark_click = len(log())
send("motion 70 100")
send("button press")
send("button release")
time.sleep(0.5)
after_click = rows()
focus1 = focus_surfaces(mark_click)
print(f"ordem do space depois de clicar em A: {after_click}")
print(f"foco após 1º clique em A: {focus1} (A={surf_a}, B={surf_b})")
if not after_click or after_click[-1][2] != Y_A:
    print("FALHA: clique no conteúdo de A não elevou A ao topo do space", file=sys.stderr)
    sys.exit(1)

# 2b) segundo clique na MESMA janela A: o foco não pode saltar para B
mark_click2 = len(log())
send("button press")
send("button release")
time.sleep(0.5)
focus2 = focus_surfaces(mark_click2)
print(f"foco após 2º clique na mesma janela A: {focus2}")

# 3) resize pela borda esquerda do frame de A. frame origin = A - insets.
cA = content_of(0)
if cA is None:
    print("FALHA: sem configure de conteúdo para window_id=0", file=sys.stderr)
    sys.exit(1)
total_h = cA[1] + INSETS[1] + INSETS[3]
fx, fy = a0[1] - INSETS[0] + 3, a0[2] - INSETS[1] + total_h // 2
mark = len(log())
send(f"motion {fx} {fy}")
send("button press")
send(f"motion {fx - 40} {fy}")
send("button release")
time.sleep(0.8)

detected = [m.group(1) for m in hits(r"borda/canto -> resize (\w+) serial=\d+", mark)]
grabbed = [m.group(1) for m in hits(r"resize do frame: grab iniciado.*?edges=(\w+)", mark)]
print(f"resize detectado={detected[-1] if detected else None} grab={grabbed[-1] if grabbed else None}")
print("log do resize:", "\n".join(
    l for l in log()[mark:].splitlines()
    if "resize" in l.lower() or "mover" in l.lower()
)[-1500:])

after = rows()
a1, b1 = by_y(Y_A), by_y(Y_B)
print(f"depois do resize: A={a1}  B={b1}")

sock.close()
cleanup()

a_moved = bool(a1 and a0 and (a1[3], a1[4]) != (a0[3], a0[4]))
b_intact = bool(b1 and b0 and (b1[3], b1[4]) == (b0[3], b0[4]))
focus_ok = bool(
    surf_a in focus1 and surf_b not in focus1 and surf_b not in focus2
)
verdict = bool(detected and grabbed and a_moved and b_intact and focus_ok)
print(f"\nA mudou={a_moved}  B intacta={b_intact}  foco só em A={focus_ok}")
print(f"VEREDITO: {'OK' if verdict else 'FALHA'}")
print("(esperado: resize do frame de A muda A, B idêntica, foco nunca em B)")
sys.exit(0 if verdict else 1)
