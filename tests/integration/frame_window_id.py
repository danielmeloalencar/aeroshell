#!/usr/bin/env python3
"""Teste de roteamento de comando por window_id (2 janelas).

Invariantes provadas:
  (1) o frame_host cria UMA janela WebView por window_id anunciado pelo compositor
      (`frame_requested`), em vez de uma única janela para "a primeira";
  (2) o ✕ do frame da janela B manda `close` para o window_id de B, não para o da
      primeira entrada (`entries.first()`);
  (3) fechar B não fecha A: `list` depois do close ainda contém a linha de A.

Geometria: o Space põe o toplevel n em (60+40n, 80+40n) (protocols/xdg_shell.rs:52).
O frame é posicionado em (janela - insets) com insets (6,30,6,6); botão `close` com
centro em (total-15, 15) no espaço HTML da WebView. `total` é lido do último configure
por window_id, porque o foot commita tamanho arredondado pela grade.

O clique é motion -> press -> release; só a linha `FRAMEWEB: IPC 'close <id>' ->`
prova que o comando saiu do handler de mensagem da WebView.
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

SOCK = f"wayland-aero-windowid-{os.getpid()}"
LOG = f"/tmp/windowid-comp-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/windowid-input-{os.getpid()}.sock"
IPC_SOCK = f"/tmp/windowid-ipc-{os.getpid()}.sock"
INSETS = (6, 30, 6, 6)

# Nunca matar por nome: isto pode encerrar a aplicação do usuário. Cada execução
# tem socket e PID próprios; o cleanup abaixo encerra só o compositor iniciado aqui.
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


def hits(pattern, since=None):
    return [m for m in re.finditer(pattern, log()) if since is None or m.start() >= since]


def wait(pattern, timeout=60, group=1):
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


def content_of(window_id):
    m = hits(
        r"frame configure \(conteúdo do cliente\).*?window_id=(\d+).*?content_w=(\d+).*?content_h=(\d+)"
    )
    m = [x for x in m if int(x.group(1)) == window_id]
    return None if not m else (int(m[-1].group(2)), int(m[-1].group(3)))


def frame_origin(window_id, entry):
    # entry do `list`: "W id x y w h" -> frame = janela - insets
    _, x, y, _, _ = entry
    return (x - INSETS[0], y - INSETS[1])


def close_button(window_id, entry):
    c = content_of(window_id)
    if c is None:
        return None
    total = (c[0] + INSETS[0] + INSETS[2], c[1] + INSETS[1] + INSETS[3])
    origin = frame_origin(window_id, entry)
    return (origin[0] + total[0] - 15, origin[1] + 15)


if not wait("injeção de ponteiro sintético ativa", 15, 0):
    print("FALHA: socket de input sintético não apareceu", file=sys.stderr)
    sys.exit(1)

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect(INPUT_SOCK)


def send(line):
    sock.sendall((line + "\n").encode())
    time.sleep(0.5)


def probe(x, y):
    mark = len(log())
    send(f"motion {x} {y}")
    m = hits(
        r"TESTE ponteiro motion x=[\d.]+ y=[\d.]+ under=Some\(\(ObjectId(wl_surface@\d+\[\d+\], (\d+)), "
        r"Point<[^>]+> \{ x: (-?[\d.]+), y: (-?[\d.]+) \}\)",
        mark,
    )
    return None if not m else (m[-1].group(1), float(m[-1].group(2)), float(m[-1].group(3)))


def click(x, y):
    mark = len(log())
    hit = probe(x, y)
    send("button press")
    send("button release")
    ipc_cmd = wait(r"FRAMEWEB: IPC '(\w+ \d+)' ->", 12, 1)
    return hit, ipc_cmd, mark


# 1) duas janelas no Space
for _ in range(int(60 * 10)):
    entries = ipc("list")
    if len(entries) >= 2:
        break
    time.sleep(0.5)
else:
    print("FALHA: só uma janela apareceu no list", file=sys.stderr)
    print(log()[-2000:], file=sys.stderr)
    sys.exit(1)

# `list` devolve "W id x y w h <titulo-json>" (window_ipc.rs:3): ignorar o título.
entries = {int(e.split()[1]): tuple(int(v) for v in e.split()[1:6]) for e in entries}
print(f"janelas no space: {sorted(entries)}")

if 0 not in entries or 1 not in entries:
    print(f"FALHA: esperava window_id 0 e 1, obtive {sorted(entries)}", file=sys.stderr)
    sys.exit(1)

# 2) o frame_host precisa ter frame para os dois window_ids
frames = {}
for m in hits(r"FRAMEWEB: associate\(window_id=(\d+)\)"):
    frames[int(m.group(1))] = m.group(0)
for _ in range(int(30 * 10)):
    if len(frames) >= 2:
        break
    frames = {int(m.group(1)): m.group(0) for m in hits(r"FRAMEWEB: associate\(window_id=(\d+)\)")}
    time.sleep(0.2)

announced = sorted(set(int(m.group(1)) for m in hits(r"FRAMEWEB: frame_requested window_id=(\d+)")))
print(f"frame_requested vistos pelo frame_host: {announced}")
print(f"associate por window_id no frame_host: {sorted(frames)}")
if len(frames) < 2:
    print("FALHA: o frame_host tem só uma WebView; não há frame para a segunda janela", file=sys.stderr)
    print(log()[-2000:], file=sys.stderr)
    sys.exit(1)

# 3) ✕ no frame da janela B (window_id 1) não pode fechar A (window_id 0)
before = sorted(entries)
point = close_button(1, entries[1])
if point is None:
    print("FALHA: sem configure de conteúdo para window_id=1", file=sys.stderr)
    sys.exit(1)
hit, cmd, mark = click(*point)
print(f"clique no ✕ do frame da janela 1 em {point}: hit_test={hit} comando_via_JS={cmd}")

closed = wait(r"FRAMEWEB: closed recebido", 20, 0)
after = [e for e in ipc("list")]
print(f"list depois do close: {after}")
print(f"closed recebido pelo frame_host: {bool(closed)}")
close_lines = [m.group(0) for m in hits(r"FRAMEWEB: IPC 'close \d+'")]
print(f"linhas de close no log: {close_lines}")

remaining = sorted(int(e.split()[1]) for e in after)
verdict = cmd == "close 1" and 0 in remaining and 1 not in remaining
print(f"\nVEREDITO: {'OK' if verdict else 'FALHA'}")
print("(esperado: close 1; window_id 0 continua no list; window_id 1 saiu)")
print(log()[-1500:], file=sys.stderr if not verdict else sys.stdout)

sock.close()
cleanup()
sys.exit(0 if verdict else 1)
