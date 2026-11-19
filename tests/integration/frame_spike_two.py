#!/usr/bin/env python3
"""Sanidade B.3 de 2 janelas empilhadas com o supervisor INTERNO (rota única).

Dois clientes azuis (mesmo processo, AERO_SPIKE_BLUE_N=2) -> window_id 0 e 1.
O compositor lança o spike como shell privilegiado (--shell) com
AERO_SPIKE_FRAME2=1: um frame por janela. Os logs do spike saem no stdout do
compositor (LOG_COMP). Prova:
  (a) dois associates + dois buffers de frame ok=true (conteúdo + insets);
  (b) ordem de render frente->trás: cada frame LOGO APÓS o seu cliente;
  (c) raise do cliente 1 mantém o frame 1 junto, abaixo do cliente 1;
  (d) teclado nunca foca um frame.
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

SOCK = "wayland-aero-b3two"
LOG_COMP = "/tmp/b3-two-comp.log"
INPUT_SOCK = "/tmp/b3-two-input.sock"

for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)

# insets do spike: (0,30,0,0); cliente 320x240 -> frame total 320x270.
TOTAL = (320, 270)

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
     "--command", f"AERO_SPIKE_BLUE_CLIENT=1 AERO_SPIKE_BLUE_N=2 {SPIKE}",
     "--shell", SPIKE,
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0",
     "--shell-env", "AERO_SPIKE_STAY=1",
     "--shell-env", "AERO_SPIKE_FRAME2=1"],
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


def wait_log(substr, timeout_s, count=1):
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if comp_log().count(substr) >= count:
            return True
        time.sleep(0.1)
    return False


def send_input(*lines):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.connect(INPUT_SOCK)
        s.sendall(("\n".join(lines) + "\n").encode())
    time.sleep(0.4)


def last_render_order():
    """Última ordem frente->trás como lista [(kind, id), ...].
    cliente loga `client@Some(ObjectId(...))`; frame loga `frame@ObjectId(...)`."""
    lines = [l for l in comp_log().splitlines() if "ordem de render" in l]
    if not lines:
        return []
    return re.findall(r"(client|frame)@(?:Some\()?ObjectId\(wl_surface@(\d+)", lines[-1])


def wait_render_order(timeout=15.0):
    """A ordem só é logada quando MUDA e o frame só entra após o import do buffer;
    espera a ordem com 2 clientes + 2 frames em vez de um sleep fixo."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        order = last_render_order()
        if len(order) == 4 and sum(k == "frame" for k, _ in order) == 2:
            return order
        time.sleep(0.2)
    return last_render_order()


def wait_order_change(prev, timeout=15.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        order = last_render_order()
        if len(order) == 4 and order != prev:
            return order
        time.sleep(0.2)
    return last_render_order()


# Esperar os DOIS toplevels (window_id 0 e 1) e seus buffers. O spike é lançado
# pelo supervisor no startup; seu loop de inicio atrasa o associate o bastante
# para os clientes azuis mapearem primeiro.
if not wait_log("novo toplevel xdg-surface mapeado", 30, count=2):
    fail("os dois clientes azuis não mapearam")
time.sleep(2)
print("dois clientes azuis com buffer; spike com 2 frames via supervisor")

# (a) dois associates + dois buffers de frame ok=true.
if not wait_log("frame associado; frame_requested+configure enviados", 30, count=2):
    fail("spike não associou os dois frames")
if not wait_log(
    f"FRAME_BUFFER_COMMIT buffer=Some(({TOTAL[0]}, {TOTAL[1]})) "
    f"expected=Some(({TOTAL[0]}, {TOTAL[1]})) ok=true",
    30, count=2,
):
    fail(f"os dois frames não commitaram buffer == expected == {TOTAL}")
print(f"OK (a): 2 associates + 2 buffers de frame {TOTAL} ok=true")

# Mapear cliente->window_id (ordem de map) e frame->window_id (associate).
clients = re.findall(
    r"toplevel mapeado no space surface=Some\(ObjectId\(wl_surface@(\d+)", comp_log()
)
frame_for = dict(re.findall(
    r"frame associado; frame_requested\+configure enviados window_id=(\d+) "
    r"frame_surface=ObjectId\(wl_surface@(\d+)", comp_log()
))
if len(clients) < 2 or "0" not in frame_for or "1" not in frame_for:
    fail(f"associações incompletas: clients={clients} frame_for={frame_for}")
client0, client1 = clients[0], clients[1]
frame0, frame1 = frame_for["0"], frame_for["1"]
print(f"cliente0=@{client0} frame0=@{frame0}; cliente1=@{client1} frame1=@{frame1}")


def assert_pairs(order, label):
    # Cada frame deve vir LOGO APÓS o seu próprio cliente (abaixo dele).
    if len(order) != 4:
        fail(f"{label}: ordem esperada com 4 elementos, veio {order}")
    for i in range(0, 4, 2):
        kind_c, id_c = order[i]
        kind_f, id_f = order[i + 1]
        if kind_c != "client" or kind_f != "frame":
            fail(f"{label}: par client/frame fora de ordem: {order}")
        expected_frame = frame0 if id_c == client0 else frame1 if id_c == client1 else None
        if expected_frame is None or id_f != expected_frame:
            fail(f"{label}: frame @{id_f} não é o do cliente @{id_c}: {order}")
    print(f"OK {label}: cada frame abaixo do seu cliente -> {order}")


# (b) ordem inicial (cliente 1 mapeado por último = topo).
order_init = wait_render_order()
assert_pairs(order_init, "(b) ordem inicial")

# (c) raise: clicar só no cliente 0 o sobe; depois só no cliente 1 o sobe.
send_input("motion 70 90", "button press", "button release")
order0 = wait_order_change(order_init)
assert_pairs(order0, "(c) raise cliente0")
if order0[0][1] != client0:
    fail(f"raise do cliente0 não o pôs no topo: {order0}")
send_input("motion 400 340", "button press", "button release")
order1 = wait_order_change(order0)
assert_pairs(order1, "(c) raise cliente1")
if order1[0][1] != client1:
    fail(f"raise do cliente1 não o pôs no topo: {order1}")

# (d) teclado nunca num frame.
focus_true = re.findall(
    r"foco de teclado alterado surface=Some\(ObjectId\(wl_surface@(\d+)[^\n]*?focus=true",
    comp_log(),
)
if frame0 in focus_true or frame1 in focus_true:
    fail(f"teclado focou um frame: focus_true={focus_true}")
print(f"OK (d): teclado nunca nos frames (@{frame0},@{frame1}); focus_true={focus_true}")

comp.terminate()
try:
    comp.wait(timeout=5)
except subprocess.TimeoutExpired:
    comp.kill()
print(f"logs: {LOG_COMP}")
print("SANIDADE 2 JANELAS: PASSOU (frame de cada uma abaixo do seu cliente)")