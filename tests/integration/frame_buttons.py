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

subprocess.run(["pkill", "-x", "frame_webview"])
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

def button_center(which):
    """Centro do botão no espaço do output, com o total atual."""
    c = content()
    total = (c[0] + INSETS[0] + INSETS[2], c[1] + INSETS[1] + INSETS[3])
    x = total[0] - (15 if which == "close" else 45)
    return (FRAME_ORIGIN[0] + x, FRAME_ORIGIN[1] + 15)

if not wait(r"frame configure \(conteúdo do cliente\)", 60):
    print("FALHA: configure do frame não chegou", file=sys.stderr)
    print(log()[-2000:], file=sys.stderr)
    sys.exit(1)
start = content()
total0 = (start[0] + INSETS[0] + INSETS[2], start[1] + INSETS[1] + INSETS[3])
print(f"conteudo inicial={start} frame_origin={FRAME_ORIGIN} total={total0}")

client = probe(CONTENT_ORIGIN[0] + start[0] // 2, CONTENT_ORIGIN[1] + start[1] // 2)
print(f"surface do cliente (probe no conteúdo) = {client[0]}")

print("\n=== maximizar (1º clique) ===")
hit, cmd, own = click(*button_center("max"))
on_frame = hit is not None and hit[1] == FRAME_ORIGIN[0] and hit[2] == FRAME_ORIGIN[1]
print(f"hit-test no botão max: {hit} hit_no_frame={on_frame} (surface do cliente={client[0]})")
print(f"frame_host viu o press={own}")
print(f"comando que chegou ao IPC via JS={cmd}")
maxed = content()
print(f"conteudo depois de maximizar={maxed} (esperado ≈ {OUTPUT_SIZE} arredondado pela grade do foot)")

print("\n=== restaurar (2º clique no mesmo botão) ===")
hit2, cmd2, own2 = click(*button_center("max"))
restored = content()
on_frame2 = hit2 is not None and hit2[1] == FRAME_ORIGIN[0] and hit2[2] == FRAME_ORIGIN[1]
print(f"hit-test no botão max: {hit2} hit_no_frame={on_frame2}")
print(f"frame_host viu o press={own2}")
print(f"comando via JS ao IPC={cmd2}; conteudo={restored}")

print("\n=== fechar ===")
hit3, cmd3, own3 = click(*button_center("close"))
on_frame3 = hit3 is not None and hit3[1] == FRAME_ORIGIN[0] and hit3[2] == FRAME_ORIGIN[1]
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
comp.terminate()
comp.wait(timeout=5)

verdict = bool(
    on_frame and cmd == "maximize" and maxed[0] > start[0] and maxed[1] > start[1]
    and cmd2 == "maximize" and restored != maxed
    and on_frame3 and cmd3 == "close" and closed and not [l for l in lines if l.startswith("W ")]
)
print(f"\nVEREDITO: {'OK' if verdict else 'FALHA'}")
sys.exit(0 if verdict else 1)
