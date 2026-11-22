#!/usr/bin/env python3
"""F1-titlebar-real-title: o #title do frameWebView deve mostrar o titulo real.

Pre-requisito: cargo build -p aeroshell-compositor && cargo build -p aeroshell-shell --example frame_webview

Sobe o nested com um `foot -T aero-title-probe`, espera a sonda
`FRAMEWEB: title_applied window_id=0 titulo=...` (valor lido de volta do DOM
pelo JS e enviado por postMessage), e repete com um segundo cliente de titulo
diferente. Baseline: o HTML traz o texto fixo "janela" e a sonda nao existe.

Fecha somente os PIDs que este harness iniciou.
Uso: python3 tests/integration/frame_title.py
"""
import os
import pathlib
import re
import signal
import socket
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path("/tmp/opencode/f1-title")
OUT.mkdir(parents=True, exist_ok=True)
SOCK = f"wayland-aero-title-{os.getpid()}"
LOG = OUT / f"comp-{os.getpid()}.log"
IPC = OUT / f"ipc-{os.getpid()}.sock"
TITLE_A = "aero-title-probe"
TITLE_B = "aero-title-segundo"

for orphan in (f"/run/user/1000/{SOCK}", str(IPC)):
    if os.path.exists(orphan):
        os.unlink(orphan)

env = os.environ.copy()
for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(RUST_LOG="debug", AERO_WINDOW_IPC_SOCK=str(IPC))

comp = subprocess.Popen(
    [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "winit",
     "--socket", SOCK, "--size", "900x600",
     "--command", f"foot -T {TITLE_A}",
     "--shell", str(ROOT / "target/debug/examples/frame_webview"),
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
    cwd=ROOT, env=env, stdout=LOG.open("wb"), stderr=subprocess.STDOUT,
)
second = None


def cleanup():
    global second
    for proc in (second, comp):
        if proc is not None and proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=5)


def text():
    return re.sub(r"\x1b\[[0-9;]*m", "", LOG.read_text(errors="replace"))


def wait_for(pattern, timeout=20.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if re.search(pattern, text()):
            return True
        if comp.poll() is not None:
            return False
        time.sleep(0.1)
    return False


def ipc_list():
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(3)
            s.connect(str(IPC))
            s.sendall(b"list\n")
            s.shutdown(socket.SHUT_WR)
            return s.recv(8192).decode(errors="replace")
    except OSError as err:
        return f"ERR {err}"


try:
    first_ok = wait_for(
        rf"FRAMEWEB: title_applied window_id=0 titulo={re.escape(TITLE_A)}\b", 30.0
    )
    print(f"PRIMEIRO titulo={TITLE_A}: {'OK' if first_ok else 'FALHA'}")
    list_a = ipc_list()
    print("IPC apos cliente A:", repr(list_a))

    second_err = OUT / f"foot2-{os.getpid()}.err"
    second = subprocess.Popen(
        ["foot", "-T", TITLE_B], cwd=ROOT,
        stdout=subprocess.DEVNULL, stderr=second_err.open("wb"),
        env={**os.environ, "WAYLAND_DISPLAY": SOCK, "GDK_BACKEND": "wayland"},
        start_new_session=True,
    )
    second_ok = wait_for(
        rf"FRAMEWEB: title_applied window_id=1 titulo={re.escape(TITLE_B)}\b", 30.0
    )
    print(f"SEGUNDO titulo={TITLE_B}: {'OK' if second_ok else 'FALHA'}")
    if second is not None:
        err_path = OUT / f"foot2-{os.getpid()}.err"
        print("STDERR foot2:", repr(err_path.read_text(errors="replace") if err_path.exists() else ""))
    print("IPC apos cliente B:", repr(ipc_list()))

    log = text()
    checks = {
        f"DOM #title = {TITLE_A}": first_ok,
        f"DOM #title = {TITLE_B} (mudanca)": second_ok,
        "IPC tem titulo A": f'"{TITLE_A}"' in ipc_list(),
        "IPC tem titulo B": f'"{TITLE_B}"' in ipc_list(),
        "associou frame 0": "FRAMEWEB: associate(window_id=0)" in log,
        "sem erro de protocolo": not re.search(r"error \d+:|protocol error", log, re.I),
    }
    for name, ok in checks.items():
        print(f"{'OK ' if ok else 'FALHA '} {name}")
    print(f"log_cru={LOG}")
    print("VEREDITO: " + ("OK" if all(checks.values()) else "FALHA"))
    sys.exit(0 if all(checks.values()) else 1)
finally:
    if second is not None and second.poll() is None:
        try:
            os.killpg(second.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    cleanup()
