#!/usr/bin/env python3
"""F1i+: globals de text_input/input_method/xdg_activation + ativação real.

Pre-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-compositor --example f1i_client
  cargo build -p aeroshell-shell --example frame_webview

Roda no headless, sem display; fecha somente os PIDs que iniciou.
Uso: python3 tests/integration/f1i_protocols.py
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
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f1i-"))
SOCK = f"wayland-aero-f1i-{os.getpid()}"
RUNTIME = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
REQUIRED = (
    "zwp_text_input_manager_v3",
    "zwp_input_method_manager_v2",
    "xdg_activation_v1",
)


def send_input(path: str, line: str):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.connect(path)
        s.sendall(f"{line}\n".encode())


def run_foot(sock: str) -> str:
    """foot real no socket: captura os avisos de globals ausentes."""
    err = OUT / "foot.err"
    env = {**os.environ, "WAYLAND_DISPLAY": sock, "GDK_BACKEND": "wayland"}
    env.pop("DISPLAY", None)
    proc = subprocess.Popen(
        ["foot"], cwd=ROOT, stdout=subprocess.DEVNULL, stderr=err.open("wb"),
        env=env, start_new_session=True,
    )
    try:
        time.sleep(4)
    finally:
        # Só o grupo de processos que EU criei: foot fork um servidor filho.
        try:
            os.killpg(proc.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            proc.wait(timeout=5)
    return err.read_text(errors="replace")


def main() -> int:
    comp_log = OUT / "compositor.log"
    client_err = OUT / "client.err"
    input_sock = str(OUT / "input.sock")
    comp = subprocess.Popen(
        [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
         "--socket", SOCK, "--shell", str(ROOT / "target/debug/examples/frame_webview"),
         "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
        cwd=ROOT, stdout=comp_log.open("wb"), stderr=subprocess.STDOUT,
        env={**os.environ, "RUST_LOG": "debug", "AERO_TEST_INPUT_SOCK": input_sock},
    )
    client = None
    try:
        deadline = time.time() + 15
        while time.time() < deadline and not (RUNTIME / SOCK).exists():
            if comp.poll() is not None:
                print("FALHA: compositor saiu antes do socket")
                return 1
            time.sleep(0.05)
        if not (RUNTIME / SOCK).exists():
            print("FALHA: socket do compositor não apareceu")
            return 1
        time.sleep(0.2)
        client = subprocess.Popen(
            [str(ROOT / "target/debug/examples/f1i_client")],
            cwd=ROOT, stdout=subprocess.DEVNULL, stderr=client_err.open("wb"),
            env={**os.environ, "WAYLAND_DISPLAY": SOCK, "GDK_BACKEND": "wayland"},
        )
        # Injeta input SOMENTE quando o cliente já tem o toplevel mapeado:
        # o serial do botão é usado no token de xdg_activation.
        deadline = time.time() + 15
        injected = False
        while time.time() < deadline and client.poll() is None and not injected:
            out = client_err.read_text(errors="replace")
            if "F1I_CLIENT pronto_para_input" in out:
                send_input(input_sock, "motion 300 300")
                time.sleep(0.1)
                send_input(input_sock, "button press")
                send_input(input_sock, "button release")
                injected = True
            time.sleep(0.05)
        deadline = time.time() + 20
        while time.time() < deadline and client.poll() is None:
            time.sleep(0.1)
        if client.poll() is None:
            client.terminate()
            client.wait(timeout=5)

        foot_out = run_foot(SOCK)
        print("=== foot ===")
        print(foot_out, end="")

        out = client_err.read_text(errors="replace")
        log = comp_log.read_text(errors="replace")
        print(out, end="")
        checks = {
            f"global {name}": f"F1I_CLIENT CHECK {name} presente" in out
            for name in REQUIRED
        }
        checks.update({
            "cliente terminou OK": "F1I_CLIENT OK" in out,
            "serial de botão recebido": "F1I_CLIENT serial_recebido" in out,
            "token de ativação recebido": "F1I_CLIENT token_recebido" in out,
            "ativação solicitada": "F1I_CLIENT activation_requested" in out,
            "handler aceitou (bench)": bool(re.search(r"XDG_ACTIVATE t=\d+ decisao=allow", log)),
            "sonda TI_FOCUS (bench)": "TI_FOCUS t=" in log,
            "foot sem aviso text input": (
                "text interface not implemented compositor" not in foot_out
                and "IME will be disabled" not in foot_out
            ),
            "foot sem aviso xdg activation": (
                "compositor does not implement xdg activation" not in foot_out
            ),
            "sem erro de protocolo": not re.search(r"error \d+:|protocol error", log, re.I),
        })
        for name, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {name}")
        print(f"log_cru={OUT}")
        print("VEREDITO: " + ("OK" if all(checks.values()) else "FALHA"))
        return 0 if all(checks.values()) else 1
    finally:
        if client is not None and client.poll() is None:
            client.terminate()
            client.wait(timeout=5)
        if comp.poll() is None:
            comp.terminate()
            comp.wait(timeout=5)


if __name__ == "__main__":
    sys.exit(main())
