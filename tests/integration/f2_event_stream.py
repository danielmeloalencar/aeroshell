#!/usr/bin/env python3
"""F2-event-stream: o shell e um assinante recebem eventos PUSH do compositor.

Baseline (sem o stream): o assinante falha ao conectar em `<ipc>.events` e o
shell nunca loga `FRAMEWEB: aeroEvent` → VEREDITO: FALHA.

Pre-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-compositor --example event_subscriber_client
  cargo build -p aeroshell-shell --example frame_webview

Uso: python3 tests/integration/f2_event_stream.py
"""
import os
import pathlib
import re
import signal
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNTIME = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-stream-"))


def wait_for(predicate, timeout=12):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


def launch_foot(sock, title, log):
    env = {**os.environ, "WAYLAND_DISPLAY": sock, "GDK_BACKEND": "wayland"}
    env.pop("DISPLAY", None)
    return subprocess.Popen(
        ["foot", "-T", title], cwd=ROOT, stdout=subprocess.DEVNULL,
        stderr=log.open("wb"), env=env, start_new_session=True,
    )


def stop(proc):
    if proc is None or proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait(timeout=5)


def main():
    sock = f"wayland-aero-f2-stream-{os.getpid()}"
    ipc = str(OUT / "ipc.sock")
    event_sock = pathlib.Path(f"{ipc}.events")
    comp_log = OUT / "comp.log"
    sub_err = OUT / "subscriber.err"
    shell_log = comp_log  # shell herda stdout do compositor
    comp = None
    sub = None
    foots = []
    try:
        env = {**os.environ, "RUST_LOG": "debug", "AERO_WINDOW_IPC_SOCK": ipc}
        for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET"):
            env.pop(key, None)
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
             "--socket", sock,
             "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, stdout=comp_log.open("wb"), stderr=subprocess.STDOUT, env=env,
        )
        deadline = time.time() + 15
        while time.time() < deadline and not (RUNTIME / sock).exists():
            time.sleep(0.05)
        # O listener de eventos é uma thread: esperar o arquivo evita corrida
        # em que o assinante conecta antes do bind.
        deadline = time.time() + 15
        while time.time() < deadline and not event_sock.exists():
            time.sleep(0.05)
        time.sleep(0.2)
        sub = subprocess.Popen(
            [str(ROOT / "target/debug/examples/event_subscriber_client")],
            cwd=ROOT, stdout=subprocess.DEVNULL, stderr=sub_err.open("wb"),
            env={**os.environ, "AERO_WINDOW_IPC_SOCK": ipc},
        )
        connected = wait_for(lambda: "EVENT_CLIENT CONECTADO" in sub_err.read_text(errors="replace"))
        foots.append(launch_foot(sock, "aero-event-A", OUT / "foot-a.err"))
        time.sleep(1.5)
        foots.append(launch_foot(sock, "aero-event-B", OUT / "foot-b.err"))
        time.sleep(3)

        out = sub_err.read_text(errors="replace")
        log = shell_log.read_text(errors="replace")
        print("=== assinante (cru) ===")
        print(out, end="")
        print("=== shell (linhas de evento) ===")
        for line in log.splitlines():
            if "aeroEvent" in line or "EVENT_PUSH" in line or "ProtocolError" in line:
                print(line)

        events = re.findall(r"EVENT_CLIENT EVENT (.+)", out)
        kinds = set()
        for event in events:
            m = re.match(r'\{"([A-Za-z]+)"', event.strip())
            if m:
                kinds.add(m.group(1))
        checks = {
            "assinante conectou no stream": connected,
            "eventos push recebidos": len(events) >= 2,
            "evento Title entregue": "Title" in kinds,
            "evento Geometry/FramePainted entregue": bool(kinds & {"Geometry", "FramePainted"}),
            "shell consumiu stream (aeroEvent)": "aeroEvent" in log,
            "sonda bench EVENT_PUSH": "EVENT_PUSH " in log,
            "zero erros de protocolo": "ProtocolError" not in log,
        }
        print(f"eventos={len(events)} kinds={sorted(kinds)}")
        for name, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {name}")
        verdict = "OK" if all(checks.values()) else "FALHA"
        print("VEREDITO: " + verdict)
        return 0 if verdict == "OK" else 1
    finally:
        for foot in foots:
            stop(foot)
        if sub is not None and sub.poll() is None:
            sub.terminate()
            sub.wait(timeout=5)
        if comp is not None and comp.poll() is None:
            comp.terminate()
            comp.wait(timeout=5)


if __name__ == "__main__":
    sys.exit(main())
