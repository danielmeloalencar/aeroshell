#!/usr/bin/env python3
"""02p: uma segunda ativação do GApplication não pode derrubar os frames.

Um segundo processo frame_webview com o mesmo application_id ativa a instância
primária via D-Bus. O handler não pode abrir/associar window_id=0 outra vez.
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
SOCK = f"wayland-aero-02p-activate-{os.getpid()}"
LOG = f"/tmp/opencode/02p-activate-comp-{os.getpid()}.log"

for orphan in (f"/run/user/1000/{SOCK}",):
    if os.path.exists(orphan):
        os.unlink(orphan)
time.sleep(1)
subprocess.run(
    ["cargo", "build", "-p", "aeroshell-compositor", "--features", "bench"],
    cwd=ROOT, check=True,
)
subprocess.run(
    ["cargo", "build", "-p", "aeroshell-shell", "--example", "frame_webview"],
    cwd=ROOT, check=True,
)
bus = subprocess.Popen(
    ["dbus-daemon", "--session", "--nofork", "--print-address=1"],
    stdout=subprocess.PIPE, text=True,
)
bus_address = bus.stdout.readline().strip()
env = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD", "WAYLAND_DISPLAY"):
    env.pop(key, None)
env.update(RUST_LOG="debug", DBUS_SESSION_BUS_ADDRESS=bus_address)
comp_log = open(LOG, "w")
comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK, "--command", "foot",
     "--shell", FRAME, "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
    env=env, stdout=comp_log, stderr=subprocess.STDOUT,
)


def cleanup():
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()
    comp_log.close()
    if bus.poll() is None:
        bus.terminate()
        try:
            bus.wait(timeout=5)
        except subprocess.TimeoutExpired:
            bus.kill()


atexit.register(cleanup)


def log():
    return re.sub(r"\x1b\[[0-9;]*m", "", open(LOG, errors="ignore").read())


def wait_for(pattern, timeout_s=30):
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if re.search(pattern, log()):
            return True
        time.sleep(0.1)
    return False


if not wait_for(r"associate\(window_id=0\)"):
    print("FALHA: associate inicial", file=sys.stderr)
    sys.exit(1)
if not wait_for(r"RESIZE_GLUE .*buffer=\d+x\d+"):
    print("FALHA: frame inicial não renderizou", file=sys.stderr)
    sys.exit(1)

second_env = env.copy()
second_env.update(
    WAYLAND_DISPLAY=SOCK,
    LD_PRELOAD="libgtk4-layer-shell.so.0",
)
second = subprocess.run(
    [FRAME], env=second_env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    text=True, timeout=20,
)
mark = len(log())
time.sleep(1.5)
text = log()
after = text[mark:]
print("--- segundo processo (cru) ---")
print(second.stdout)
print("--- ativação/erros (cru) ---")
for line in text.splitlines():
    if any(token in line for token in (
        "activate", "associate(window_id=0)", "já tem frame", "privilegiada do shell caiu",
    )):
        print(line)

ignored = "FRAMEWEB: activate repetido ignorado" in text
duplicate = "janela 0 já tem frame" in text
shell_died = "conexão privilegiada do shell caiu" in text
associates = len(re.findall(r"associate\(window_id=0\)", text))
new_frames = len(re.findall(r"RESIZE_GLUE .*buffer=\d+x\d+", after))
print(f"activate_ignorado={ignored} duplicate={duplicate} shell_died={shell_died} "
      f"associates={associates} frames_depois={new_frames}")
problems = []
if not ignored:
    problems.append("ativação repetida não foi ignorada")
if duplicate:
    problems.append("segunda ativação tentou associar window_id=0 novamente")
if shell_died:
    problems.append("conexão privilegiada morreu")
if associates != 1:
    problems.append(f"associates window_id=0 = {associates}, esperado 1")
if not new_frames:
    problems.append("frame deixou de renderizar após a segunda ativação")
if problems:
    print("VEREDITO: FALHA")
    for problem in problems:
        print("  " + problem)
    sys.exit(1)
print("VEREDITO: OK")
