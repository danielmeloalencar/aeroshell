#!/usr/bin/env python3
"""F1d3: provar que o elemento do cliente é criado na cena compartilhada.

A linha ``ordem de render … client@…`` de ``render/scene.rs`` é empurrada
incondicionalmente e NÃO prova que ``Window::render_elements`` devolveu algo.
Este harness exige, sob ``bench``, a sonda ``CLIENT_ELEMENT_PROBE`` com:

    delta = elements.len() depois - elements.len() antes (> 0)
    from_surface = Some  (o import da superfície do cliente gerou elemento)
    err = none           (nenhum erro de import)

Roda no backend headless com ``foot`` real e o mesmo ``render/scene.rs`` do
udev. Não julga pixels nem a tela do tty3; só fecha a lacuna do log.
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

SOCK = f"wayland-aero-f1d3-element-{os.getpid()}"
OUT = "/tmp/opencode/f1d3-pixels"
LOG = os.path.join(OUT, f"element-{os.getpid()}.log")
os.makedirs(OUT, exist_ok=True)
socket_path = f"/run/user/1000/{SOCK}"
if os.path.exists(socket_path):
    os.unlink(socket_path)

subprocess.run(
    ["cargo", "build", "-p", "aeroshell-compositor", "--features", "bench"],
    cwd=ROOT, check=True, stdout=subprocess.DEVNULL,
)
subprocess.run(
    ["cargo", "build", "-p", "aeroshell-shell", "--example", "frame_webview"],
    cwd=ROOT, check=True, stdout=subprocess.DEVNULL,
)

# Barramento privado: a sessão udev do humano pode manter o application_id do
# shell ocupado no barramento real.
bus = subprocess.Popen(
    ["dbus-daemon", "--session", "--nofork", "--print-address=1"],
    stdout=subprocess.PIPE, text=True,
)
bus_address = bus.stdout.readline().strip()

env = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(RUST_LOG="info", DBUS_SESSION_BUS_ADDRESS=bus_address)
log_file = open(LOG, "w")
comp = subprocess.Popen(
    [COMP, "--backend", "headless", "--socket", SOCK,
     "--size", "1920x955", "--shell", FRAME,
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0",
     "--command", "foot -w 696x494"],
    env=env, stdout=log_file, stderr=subprocess.STDOUT,
)


@atexit.register
def cleanup():
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()
    log_file.close()
    if bus.poll() is None:
        bus.terminate()
        try:
            bus.wait(timeout=5)
        except subprocess.TimeoutExpired:
            bus.kill()


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


problems = []
if not wait_for(r"toplevel mapeado no space", label="cliente foot mapeado"):
    sys.exit(1)
if not wait_for(r"RENDER_SHARED window_id=0 kind=frame", label="frame WebKit"):
    sys.exit(1)
wait_for(r"CLIENT_ELEMENT_PROBE", timeout_s=5.0, label="CLIENT_ELEMENT_PROBE")

time.sleep(1.0)
lines = [line for line in log().splitlines() if "CLIENT_ELEMENT_PROBE" in line]
parsed = []
malformed = []
for line in lines:
    match = re.search(
        r"CLIENT_ELEMENT_PROBE .* from_surface=(\S+) err=(\S+) "
        r"before=(\d+) after=(\d+) delta=(\d+)",
        line,
    )
    if not match:
        malformed.append(line)
        continue
    state, err, before, after, delta = match.groups()
    parsed.append({
        "raw": line,
        "state": state,
        "err": err,
        "before": int(before),
        "after": int(after),
        "delta": int(delta),
    })

print(f"LINHAS_CLIENT_ELEMENT_PROBE={len(lines)} parseadas={len(parsed)} "
      f"malformadas={len(malformed)}")
print("--- cruas (primeiras 5) ---")
for line in lines[:5]:
    print("  " + line)
print("--- cruas (últimas 5) ---")
for line in lines[-5:]:
    print("  " + line)

if not lines:
    problems.append("nenhuma linha CLIENT_ELEMENT_PROBE (sonda ausente)")
if malformed:
    problems.append(f"{len(malformed)} linhas CLIENT_ELEMENT_PROBE malformadas")
if parsed:
    max_delta = max(item["delta"] for item in parsed)
    some = sum(item["state"] == "Some" for item in parsed)
    none = sum(item["state"] == "None" for item in parsed)
    errs = sum(item["err"] != "none" for item in parsed)
    zero = sum(item["delta"] == 0 for item in parsed)
    print(f"max_delta={max_delta} from_surface_Some={some} None={none} "
          f"erros_import={errs} delta_zero={zero}")
    if max_delta < 1:
        problems.append("delta de elementos do cliente nunca foi > 0")
    if some == 0:
        problems.append("from_surface nunca devolveu Some")
    if errs:
        problems.append(f"{errs} erros de import da superfície do cliente")
else:
    print("max_delta=0 from_surface_Some=0 None=0 erros_import=0 delta_zero=0")

print(f"log_completo={LOG}")
if problems:
    for problem in problems:
        print("  FALHA: " + problem)
    print("VEREDITO: FALHA — criação do elemento do cliente NÃO provada")
    sys.exit(1)
print("VEREDITO: OK — elemento do cliente criado e importado na cena compartilhada")
