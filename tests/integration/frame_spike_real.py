#!/usr/bin/env python3
"""Gate B.3 com o supervisor INTERNO do compositor (rota única).

O compositor lança o frame_spike como shell privilegiado (--shell): o socketpair
é criado dentro do compositor, sem AERO_PRIVILEGED_FD nem socketpair externo.
O cliente azul é um cliente PÚBLICO via --command. Os logs do spike (filho) saem
no stdout do compositor, então tudo é lido de LOG_COMP.

Gate de tamanho: FRAME_BUFFER_COMMIT buffer == expected em DOIS tamanhos;
depois move/resize com serial válido, teclado nunca no frame, e closed.
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
INPUT_SOCK = "/tmp/b3-input.sock"

for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)

SIZE_1 = (320, 270)   # cliente 320x240 + titlebar 30
SIZE_2 = (480, 350)   # cliente 480x320 + titlebar 30

base = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    base.pop(key, None)

cenv = base.copy()
cenv.update(
    WAYLAND_DISPLAY=base.get("WAYLAND_DISPLAY", "wayland-0"),
    RUST_LOG="debug",
    AERO_TEST_INPUT_SOCK=INPUT_SOCK,
)
comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK,
     "--command", f"AERO_SPIKE_BLUE_CLIENT=1 AERO_SPIKE_RESIZE=480x320 {SPIKE}",
     "--shell", SPIKE,
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0",
     "--shell-env", "AERO_SPIKE_STAY=1"],
    env=cenv,
    stdout=open(LOG_COMP, "w"), stderr=subprocess.STDOUT,
)
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
        comp.wait(timeout=5)
    except subprocess.TimeoutExpired:
        comp.kill()
    print(f"logs: {LOG_COMP}")
    sys.exit(1)


# Evitar a corrida: o window_id 0 só existe depois que o cliente mapeia o toplevel.
if "novo toplevel xdg-surface mapeado" not in comp_log():
    deadline = time.time() + 15
    while time.time() < deadline and "novo toplevel xdg-surface mapeado" not in comp_log():
        time.sleep(0.1)
if "novo toplevel xdg-surface mapeado" not in comp_log():
    fail("cliente azul não mapeou antes do associate")
time.sleep(2)
print("cliente azul com buffer; spike privilegiado lançado pelo supervisor")

if os.environ.get("AERO_SPIKE_MANUAL"):
    print(f"MANUAL: compositor pid={comp.pid} (spike é filho privilegiado)")
    print(f"logs: {LOG_COMP}")
    print("NAO encerrar: usuario testa e fecha a janela do compositor.")
    raise SystemExit(0)

COMMIT_RE = re.compile(
    r"FRAME_BUFFER_COMMIT buffer=Some\(\((\d+), (\d+)\)\) "
    r"expected=Some\(\((\d+), (\d+)\)\) ok=(true|false)"
)


def wait_gate(size, timeout_s):
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
frame_id = re.search(r"frame_surface=ObjectId\(wl_surface@(\d+)", comp_log())
client_id = re.search(r"toplevel mapeado no space surface=Some\(ObjectId\(wl_surface@(\d+)", comp_log())
if not frame_id or not client_id:
    fail("não identifiquei as superfícies do frame/cliente no log")
frame_id, client_id = frame_id.group(1), client_id.group(1)
print(f"superfícies: cliente=@{client_id} frame=@{frame_id}")

send_input("motion 200 60", "button press", "button release")
if not wait_log("move do frame", 5):
    fail("move com serial válido não foi aceito pelo compositor")
if "-> move serial=" not in comp_log():
    fail("spike não enviou move a partir do clique na titlebar")
print("OK move: clique real na titlebar -> serial válido aceito")

send_input("motion 530 60", "button press", "button release")
if not wait_log("resize do frame", 5):
    fail("resize com serial válido não foi aceito pelo compositor")
if "-> resize serial=" not in comp_log():
    fail("spike não enviou resize a partir do clique na alça")
print("OK resize: clique real na alça -> serial válido aceito")

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

if "closed recebido" not in comp_log():
    fail("spike não recebeu closed após morte do cliente")
if "janela cliente destruída: closed enviado ao frame" not in comp_log():
    fail("compositor não logou closed via hook de destruição")
print("OK closed: compositor enviou e spike recebeu")

comp.terminate()
try:
    comp.wait(timeout=5)
except subprocess.TimeoutExpired:
    comp.kill()

print("=== SPIKE (privilegiado, filho do supervisor) ===")
for line in comp_log().splitlines():
    if any(k in line for k in ("SPIKE", "BLOQUEIO", "global")):
        print(line.rstrip())
print("=== COMPOSITOR (frame/commit/closed) ===")
for line in comp_log().splitlines():
    if any(k in line for k in ("associ", "frame", "FRAME_BUFFER_COMMIT", "closed", "duplo", "reconfig")):
        print(line.rstrip())
print(f"logs: {LOG_COMP}")
print("GATE DE TAMANHO: PASSOU (2 tamanhos + closed)")