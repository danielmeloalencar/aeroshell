#!/usr/bin/env python3
"""Supervisor do spike B.3: socketpair real -> compositor + frame_spike.

Gate de tamanho (B.3): FRAME_BUFFER_COMMIT buffer == expected (conteúdo +
insets + shadow) em DOIS tamanhos diferentes — o cliente azul se redimensiona
sozinho (AERO_SPIKE_RESIZE) para provar que o frame SEGUE resize.
Depois mata o cliente azul para provar closed -> frame destrói.
"""
import os
import re
import socket
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/debug/aeroshell-compositor")
SPIKE = os.path.join(ROOT, "target/debug/examples/frame_spike")

SOCK = "wayland-aero-b3"
LOG_COMP = "/tmp/b3-real-comp.log"
LOG_SPIKE = "/tmp/b3-real-spike.log"
INPUT_SOCK = "/tmp/b3-input.sock"

# Sockets órfãos de execuções anteriores do PRÓPRIO teste (dívida (d) do
# compositor: ele não remove o socket ao sair). Nomes exclusivos do teste.
for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)

# insets do spike: (0, 30, 0, 0); shadow: 0.
SIZE_1 = (320, 270)   # cliente 320x240 + titlebar 30
SIZE_2 = (480, 350)   # cliente 480x320 + titlebar 30

base = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    base.pop(key, None)

a, b = socket.socketpair()
cfd, sfd = a.fileno(), b.fileno()

cenv = base.copy()
cenv.update(
    AERO_PRIVILEGED_FD=str(cfd),
    WAYLAND_DISPLAY=base.get("WAYLAND_DISPLAY", "wayland-0"),
    RUST_LOG="debug",
    AERO_TEST_INPUT_SOCK=INPUT_SOCK,
)
comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK,
     "--command", f"AERO_SPIKE_BLUE_CLIENT=1 AERO_SPIKE_RESIZE=480x320 {SPIKE}"],
    env=cenv, pass_fds=(cfd,),
    stdout=open(LOG_COMP, "w"), stderr=subprocess.STDOUT,
)
a.close()
for _ in range(100):
    if os.path.exists(f"/run/user/1000/{SOCK}"):
        break
    time.sleep(0.05)


def comp_log():
    return re.sub(r"\x1b\[[0-9;]*m", "", open(LOG_COMP, errors="ignore").read())


def fail(msg):
    print(f"FALHA: {msg}")
    comp.terminate()
    try:
        spike.wait(timeout=3)
    except Exception:
        spike.kill()
    try:
        comp.wait(timeout=5)
    except subprocess.TimeoutExpired:
        comp.kill()
    print(f"logs: {LOG_COMP} {LOG_SPIKE}")
    sys.exit(1)


# Evitar a corrida: o window_id 0 só existe depois que o cliente mapeia o toplevel.
for _ in range(150):
    if "novo toplevel xdg-surface mapeado" in comp_log():
        break
    time.sleep(0.1)
if "novo toplevel xdg-surface mapeado" not in comp_log():
    fail("cliente azul não mapeou antes do associate")
# Esperar o cliente azul comitar buffer (geometry 0x0 até o primeiro commit).
time.sleep(2)
print("cliente azul com buffer; lançando spike")

senv = base.copy()
senv.update(
    WAYLAND_SOCKET=str(sfd),
    WAYLAND_DISPLAY=SOCK,
    GDK_BACKEND="wayland",
    LD_PRELOAD="libgtk4-layer-shell.so.0",
    AERO_SPIKE_STAY="1",
)
spike = subprocess.Popen(
    [SPIKE], env=senv, pass_fds=(sfd,),
    stdout=open(LOG_SPIKE, "w"), stderr=subprocess.STDOUT,
)
b.close()

if os.environ.get("AERO_SPIKE_MANUAL"):
    print(f"MANUAL: compositor pid={comp.pid} spike pid={spike.pid}")
    print(f"logs: {LOG_COMP} {LOG_SPIKE}")
    print("NAO encerrar: usuario testa e fecha a janela do compositor.")
    raise SystemExit(0)

COMMIT_RE = re.compile(
    r"FRAME_BUFFER_COMMIT buffer=Some\(\((\d+), (\d+)\)\) "
    r"expected=Some\(\((\d+), (\d+)\)\) ok=(true|false)"
)


def wait_gate(size, timeout_s):
    """Espera um commit com buffer == expected == size e ok=true."""
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        for m in COMMIT_RE.finditer(comp_log()):
            buf = (int(m.group(1)), int(m.group(2)))
            exp = (int(m.group(3)), int(m.group(4)))
            if buf == size and exp == size and m.group(5) == "true":
                return m.group(0)
        time.sleep(0.2)
    return None


def wait_log(substr, timeout_s):
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if substr in comp_log():
            return True
        time.sleep(0.1)
    return False


def send_input(*lines):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.connect(INPUT_SOCK)
        s.sendall(("\n".join(lines) + "\n").encode())
    time.sleep(0.4)


print(f"gate 1: esperando buffer == expected == {SIZE_1}")
line1 = wait_gate(SIZE_1, 30)
if not line1:
    fail(f"gate 1 não passou em 30 s (esperado {SIZE_1})")
print(f"OK gate 1: {line1}")

print(f"gate 2: esperando resize do cliente azul -> {SIZE_2}")
line2 = wait_gate(SIZE_2, 30)
if not line2:
    fail(f"gate 2 não passou em 30 s (esperado {SIZE_2}; frame não seguiu resize?)")
print(f"OK gate 2: {line2}")

# --- move/resize com serial válido + teclado nunca no frame ---
# Cliente 480x320 em (60,80); frame total 480x350 em (60,50); titlebar y∈[50,80).
# bbox do cliente (sombra CSD) começa em y=68, então y=60 cai FORA do bbox e o
# hit-test roteia para a superfície do frame. Alça de resize = 16 px à direita.
frame_id = re.search(r"frame_surface=ObjectId\(wl_surface@(\d+)", comp_log())
client_id = re.search(r"toplevel mapeado no space surface=Some\(ObjectId\(wl_surface@(\d+)", comp_log())
if not frame_id or not client_id:
    fail("não identifiquei as superfícies do frame/cliente no log")
frame_id, client_id = frame_id.group(1), client_id.group(1)
print(f"superfícies: cliente=@{client_id} frame=@{frame_id}")

# move: clique na titlebar fora da alça (local x=140 < 464).
send_input("motion 200 60", "button press", "button release")
if not wait_log("move do frame", 5):
    fail("move com serial válido não foi aceito pelo compositor")
if "-> move serial=" not in open(LOG_SPIKE, errors="ignore").read():
    fail("spike não enviou move a partir do clique na titlebar")
print("OK move: clique real na titlebar -> serial válido aceito")

# resize: clique nos 16 px à direita da titlebar (local x=470 > 464).
send_input("motion 530 60", "button press", "button release")
if not wait_log("resize do frame", 5):
    fail("resize com serial válido não foi aceito pelo compositor")
if "-> resize serial=" not in open(LOG_SPIKE, errors="ignore").read():
    fail("spike não enviou resize a partir do clique na alça")
print("OK resize: clique real na alça -> serial válido aceito")

# foco de teclado: clicar no conteúdo foca o CLIENTE; nunca o frame.
send_input("motion 200 200", "button press", "button release")
time.sleep(0.4)
focus_true = re.findall(
    r"foco de teclado alterado surface=Some\(ObjectId\(wl_surface@(\d+)[^\n]*?focus=true",
    comp_log(),
)
if frame_id in focus_true:
    fail(f"teclado focou o frame (@{frame_id}) — proibido")
if client_id not in focus_true:
    fail(f"clique no conteúdo não focou o cliente (@{client_id}); focus_true={focus_true}")
print(f"OK teclado: foco foi ao cliente @{client_id}, nunca ao frame @{frame_id}")

# Prova de closed: matar o cliente azul (filho real do sh -c, não o sh).
m = re.findall(r"cliente inicial iniciado.*?pid[=: ]+(\d+)", comp_log())
if m:
    sh_pid = int(m[-1])
    children = subprocess.run(
        ["ps", "-o", "pid=", "--ppid", str(sh_pid)],
        capture_output=True, text=True,
    ).stdout.split()
    for child in children or [str(sh_pid)]:
        print(f"matando cliente azul pid={child}")
        try:
            os.kill(int(child), 15)
        except ProcessLookupError:
            pass
    time.sleep(1)

slog = open(LOG_SPIKE, errors="ignore").read()
if "closed recebido" not in slog:
    fail("spike não recebeu closed após morte do cliente")
if "janela cliente destruída: closed enviado ao frame" not in comp_log():
    fail("compositor não logou closed via hook de destruição")
print("OK closed: compositor enviou e spike recebeu")

try:
    spike.wait(timeout=3)
except subprocess.TimeoutExpired:
    spike.kill()
time.sleep(0.5)
comp.terminate()
try:
    comp.wait(timeout=5)
except subprocess.TimeoutExpired:
    comp.kill()

print("=== SPIKE (privilegiado) ===")
for line in open(LOG_SPIKE, errors="ignore"):
    if any(k in line for k in ("SPIKE", "BLOQUEIO", "global")):
        print(line.rstrip())
print("=== COMPOSITOR (frame/commit/closed) ===")
for line in open(LOG_COMP, errors="ignore"):
    if any(k in line for k in ("associ", "frame", "FRAME_BUFFER_COMMIT", "closed", "duplo", "reconfig")):
        print(line.rstrip())
print(f"logs: {LOG_COMP} {LOG_SPIKE}")
print("GATE DE TAMANHO: PASSOU (2 tamanhos + closed)")