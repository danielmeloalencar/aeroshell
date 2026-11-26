#!/usr/bin/env python3
"""Passo 2: botões da barra HTML (maximizar/restaurar e fechar).

Geometria determinística com --size 800x600: o Space põe o toplevel n=0 em (60,80)
(protocols/xdg_shell.rs:52) e o frame_webview declara insets (6,30,6,6), então o frame
nasce em (54,50). Layout HTML da barra: #bar tem 30 px de altura, botões 30x30 à
direita, `max` antes de `close`; centros em (total-45, 15) e (total-15, 15).

Cada clique é motion -> press -> release no ponto do botão. Duas coisas diferentes
são medidas e registradas separadamente:
  (a) o hit-test resolve o ponto para o surface do frame (não do cliente);
  (b) o clique chegou ao JavaScript da WebView: só a linha `FRAMEWEB: IPC '<cmd>' ->`
      prova isso, porque ela é emitida pelo handler de mensagem da WebView, não pelo
      handler de ponteiro do frame_host (que imprime `FRAMEWEB: button local=`).
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

SOCK = f"wayland-aero-buttons-{os.getpid()}"
LOG = f"/tmp/buttons-comp-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/buttons-input-{os.getpid()}.sock"
IPC_SOCK = f"/tmp/buttons-ipc-{os.getpid()}.sock"
OUTPUT_SIZE = (800, 600)
INSETS = (6, 30, 6, 6)
CONTENT_ORIGIN = (60, 80)
FRAME_ORIGIN = (CONTENT_ORIGIN[0] - INSETS[0], CONTENT_ORIGIN[1] - INSETS[1])

# Nunca matar por nome: isto pode encerrar a aplicação do usuário. Cada execução
# tem socket e PID próprios; o cleanup abaixo encerra só o compositor iniciado aqui.
for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK, IPC_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)
time.sleep(1)

env = os.environ.copy()
for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(RUST_LOG="debug", AERO_TEST_INPUT_SOCK=INPUT_SOCK, AERO_WINDOW_IPC_SOCK=IPC_SOCK)

comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK, "--size", f"{OUTPUT_SIZE[0]}x{OUTPUT_SIZE[1]}",
     "--command", "foot", "--shell", FRAME,
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

def wait(pattern, timeout=45, group=0):
    for _ in range(int(timeout * 10)):
        found = hits(pattern)
        if found:
            return found[-1].group(group)
        time.sleep(0.1)
    return None

if not wait("injeção de ponteiro sintético ativa", 15):
    print("FALHA: socket de input sintético não apareceu", file=sys.stderr)
    sys.exit(1)

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect(INPUT_SOCK)

def send(line):
    sock.sendall((line + "\n").encode())
    time.sleep(0.5)

def content():
    m = hits(r"frame configure \(conteúdo do cliente\).*?content_w=(\d+).*?content_h=(\d+)")
    return None if not m else (int(m[-1].group(1)), int(m[-1].group(2)))

def content_changed(previous, timeout=12):
    """Espera o commit/configure efetivo, ignorando o refresh de estado com o mesmo tamanho."""
    for _ in range(int(timeout * 10)):
        current = content()
        if current is not None and current != previous:
            return current
        time.sleep(0.1)
    return content()


def frame_origin():
    """Origem global do frame = origem do cliente no IPC - insets esq/sup.
    02r: maximizar reposiciona a janela; a origem Não é mais a constante (54,50)."""
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(3)
    s.connect(IPC_SOCK)
    s.sendall(b"list\n")
    data = s.recv(4096).decode(errors="ignore")
    s.close()
    for line in data.splitlines():
        parts = line.split()
        if len(parts) >= 6 and parts[0] == "W" and parts[1] == "0":
            return (int(parts[2]) - INSETS[0], int(parts[3]) - INSETS[1])
    return FRAME_ORIGIN

def probe(x, y):
    mark = len(log())
    send(f"motion {x} {y}")
    m = hits(
        r"TESTE ponteiro motion x=[\d.]+ y=[\d.]+ under=Some\(\(ObjectId\(wl_surface@\d+\[\d+\], (\d+)\), "
        r"Point<[^>]+> \{ x: (-?[\d.]+), y: (-?[\d.]+) \}\)",
        mark,
    )
    return None if not m else (m[-1].group(1), float(m[-1].group(2)), float(m[-1].group(3)))

def click(x, y):
    """Clica no ponto (press + release; o DOM click só dispara no release) e devolve
    (hit_test, comando_que_chegou_ao_IPC_via_JS, press_visto_pelo_handler_de_ponteiro_do_frame_host)."""
    mark = len(log())
    hit = probe(x, y)
    send("button press")
    send("button release")
    ipc = wait(r"FRAMEWEB: IPC '(\w+) \d+' -> (\S+)", 12, 1)
    own = bool(hits(r"FRAMEWEB: button local=", mark))
    return hit, ipc, own

def button_center(which, origin):
    """Centro do botão no espaço do output, com o total e a origem atuais."""
    c = content()
    total = (c[0] + INSETS[0] + INSETS[2], c[1] + INSETS[1] + INSETS[3])
    x = total[0] - (15 if which == "close" else 45)
    return (origin[0] + x, origin[1] + 15)

if not wait(r"frame configure \(conteúdo do cliente\)", 60):
    print("FALHA: configure do frame não chegou", file=sys.stderr)
    print(log()[-2000:], file=sys.stderr)
    sys.exit(1)
start = content()
total0 = (start[0] + INSETS[0] + INSETS[2], start[1] + INSETS[1] + INSETS[3])
print(f"conteudo inicial={start} frame_origin={FRAME_ORIGIN} total={total0}")

client = probe(CONTENT_ORIGIN[0] + start[0] // 2, CONTENT_ORIGIN[1] + start[1] // 2)
print(f"surface do cliente (probe no conteúdo) = {client[0]}")

CANVAS = (OUTPUT_SIZE[0] + INSETS[0] + INSETS[2], OUTPUT_SIZE[1] + INSETS[1] + INSETS[3])


def on_frame_hit(hit, total, origin):
    """No cap direito, a origem do ponteiro é target→canvas (02p)."""
    expected = (origin[0] - (CANVAS[0] - total[0]), origin[1])
    return (
        hit is not None and hit[0] != client[0]
        and hit[1] == float(expected[0]) and hit[2] == float(expected[1])
    )

print("\n=== maximizar (1º clique) ===")
origin0 = frame_origin()
hit, cmd, own = click(*button_center("max", origin0))
on_frame = on_frame_hit(hit, total0, origin0)
print(f"hit-test no botão max: {hit} hit_no_frame={on_frame} (surface do cliente={client[0]})")
print(f"frame_host viu o press={own}")
print(f"comando que chegou ao IPC via JS={cmd}")
maxed = content_changed(start)
print(f"conteudo depois de maximizar={maxed} (esperado ≈ {OUTPUT_SIZE} arredondado pela grade do foot)")

print("\n=== restaurar (2º clique no mesmo botão) ===")
origin1 = frame_origin()  # 02r: o maximize reposicionou o frame
hit2, cmd2, own2 = click(*button_center("max", origin1))
restored = content_changed(maxed)
total_max = (maxed[0] + INSETS[0] + INSETS[2], maxed[1] + INSETS[1] + INSETS[3])
on_frame2 = on_frame_hit(hit2, total_max, origin1)
print(f"hit-test no botão max: {hit2} hit_no_frame={on_frame2}")
print(f"frame_host viu o press={own2}")
print(f"comando via JS ao IPC={cmd2}; conteudo={restored}")

print("\n=== fechar ===")
origin2 = frame_origin()
hit3, cmd3, own3 = click(*button_center("close", origin2))
total_restored = (restored[0] + INSETS[0] + INSETS[2], restored[1] + INSETS[1] + INSETS[3])
on_frame3 = on_frame_hit(hit3, total_restored, origin2)
print(f"hit-test no botão close: {hit3} hit_no_frame={on_frame3}")
print(f"frame_host viu o press={own3}")
print(f"comando via JS ao IPC={cmd3}")

closed = wait(r"FRAMEWEB: closed recebido", 20)
ipc = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
ipc.connect(IPC_SOCK)
ipc.sendall(b"list\n")
lines = [l for l in ipc.recv(4096).decode().splitlines() if l]
ipc.close()
print(f"list via IPC depois do close: {lines}")

sock.close()
cleanup()

# 02r: sem painel neste harness a zona útil é o output inteiro. O maximize agora
# ajusta o FRAME (cliente + insets) a essa zona — antes ele crescia além dela —
# e o desmaximizar devolve a geometria original. A origem do frame também muda.
frame_fits_output = (
    abs(total_max[0] - OUTPUT_SIZE[0]) <= 16 and abs(total_max[1] - OUTPUT_SIZE[1]) <= 20
)
restored_to_start = all(abs(a - b) <= 8 for a, b in zip(restored, start))
print(f"origens do frame: {origin0} -> {origin1} -> {origin2}; "
      f"alvo_max={total_max} cabe_no_output={frame_fits_output}; restaurou={restored_to_start}")
verdict = bool(
    on_frame and cmd == "maximize" and maxed != start and frame_fits_output
    and cmd2 == "maximize" and restored_to_start
    and on_frame3 and cmd3 == "close" and closed and not [l for l in lines if l.startswith("W ")]
)
print(f"\nVEREDITO: {'OK' if verdict else 'FALHA'}")
sys.exit(0 if verdict else 1)
