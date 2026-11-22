#!/usr/bin/env python3
"""02s: duas instâncias AeroShell no mesmo D-Bus não podem roubar a decoração.

Sintoma (STATE.md §6): com uma instância nested aberta, o `frame_webview` de uma
segunda instância morria no startup (0 linhas FRAMEWEB, `conexão privilegiada do
shell caiu`) e a janela aparecia sem decoração. O `application_id` fixo do GTK
fazia o novo GApplication apenas reativar a instância antiga no barramento.

O harness lança DOIS compositores nested (A e B) no MESMO D-Bus de sessão,
com sockets/IPCs/logs próprios, e exige que AMBOS cheguem a
`FRAMEWEB: manager global` + `frame_painted` e tenham `W 0 ...` no IPC.

Uma execução:
  cargo build --workspace --examples
  python3 tests/integration/frame_two_instances.py
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

OUTDIR = "/tmp/opencode/02s"
os.makedirs(OUTDIR, exist_ok=True)
RUN = os.getpid()

procs = []
handles = []


def stop_all():
    for proc in reversed(procs):
        if proc.poll() is None:
            proc.terminate()
    for proc in reversed(procs):
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
    for handle in handles:
        handle.close()


atexit.register(stop_all)


def spawn(tag):
    sock = f"wayland-aero-2inst-{tag}-{RUN}"
    log_path = os.path.join(OUTDIR, f"{tag}-{RUN}.log")
    ipc_path = os.path.join(OUTDIR, f"{tag}-ipc-{RUN}.sock")
    for orphan in (f"/run/user/{os.getuid()}/{sock}", ipc_path):
        if os.path.exists(orphan):
            os.unlink(orphan)

    env = os.environ.copy()
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
        env.pop(key, None)
    env.update(RUST_LOG="debug", AERO_WINDOW_IPC_SOCK=ipc_path)

    handle = open(log_path, "w")
    handles.append(handle)
    proc = subprocess.Popen(
        [COMP, "--backend", "winit", "--socket", sock, "--size", "800x600",
         "--command", "foot", "--shell", FRAME,
         "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
        env=env, stdout=handle, stderr=subprocess.STDOUT,
    )
    procs.append(proc)
    return {"tag": tag, "sock": sock, "log": log_path, "ipc": ipc_path, "proc": proc}


def text(inst):
    return re.sub(r"\x1b\[[0-9;]*m", "", open(inst["log"], errors="ignore").read())


def wait_for(predicate, seconds=40):
    deadline = time.time() + seconds
    while time.time() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.2)
    return None


def healthy(inst):
    t = text(inst)
    return (
        "FRAMEWEB: manager global" in t
        and "frame_painted" in t
        and "conexão privilegiada do shell caiu" not in t
    )


def ipc_list(inst):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(3)
    s.connect(inst["ipc"])
    s.sendall(b"list\n")
    data = b""
    while b"." not in data:
        chunk = s.recv(4096)
        if not chunk:
            break
        data += chunk
    s.close()
    return data.decode(errors="ignore")


def ipc_has_window(inst):
    try:
        return any(line.startswith("W ") for line in ipc_list(inst).splitlines())
    except OSError:
        return False


a = spawn("A")
if not wait_for(lambda: healthy(a) and ipc_has_window(a), 60):
    print("FALHA: instância A não ficou saudável", file=sys.stderr)
    print(text(a)[-2500:], file=sys.stderr)
    sys.exit(1)
print(f"A pronta: FRAMEWEB={text(a).count('FRAMEWEB: manager global')} "
      f"frame_painted={text(a).count('frame_painted')}")

b = spawn("B")
wait_for(lambda: healthy(b) and ipc_has_window(b), 45)

for inst in (a, b):
    t = text(inst)
    try:
        raw = ipc_list(inst).replace("\n", "|")
    except OSError as err:
        raw = f"ERRO {err}"
    print(f"{inst['tag']}: FRAMEWEB={t.count('FRAMEWEB: manager global')} "
          f"frame_painted={t.count('frame_painted')} "
          f"caiu={t.count('conexão privilegiada do shell caiu')} "
          f"ipc={raw!r} vivo={inst['proc'].poll() is None}")

verdict = all(
    healthy(inst) and ipc_has_window(inst) and inst["proc"].poll() is None
    for inst in (a, b)
)
print(f"\nVEREDITO: {'OK' if verdict else 'FALHA'}")
sys.exit(0 if verdict else 1)
