#!/usr/bin/env python3
"""F1-clipboard-two-clients: A oferece texto, B lê, com foco A→B por input sintético.

Pre-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-compositor --example clipboard_client

Modo padrão: espera `CLIP_A SELECTION_SET` antes de mover o foco para B.
`--no-sync` reproduz a corrida do instrumento que fez o baseline falhar
(B recebia `selection(nil)` antes de A definir a seleção).

Uso:
  python3 tests/integration/f1_clipboard_two_clients.py [--no-sync]
"""
import os
import json
import pathlib
import socket
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNTIME = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-clipboard-"))
TEXT = "aero-clipboard-cross-client-2026"


def send_input(path, line):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.connect(path)
        s.sendall(f"{line}\n".encode())


def query_windows(ipc_path):
    """Lê `list` do IPC: título → (x, y, w, h), sem depender da ordem de map."""
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(5)
        s.connect(ipc_path)
        s.sendall(b"list\n")
        buf = b""
        while b".\n" not in buf:
            chunk = s.recv(4096)
            if not chunk:
                break
            buf += chunk
    windows = {}
    for line in buf.decode(errors="replace").splitlines():
        parts = line.split(maxsplit=6)
        if len(parts) < 7 or parts[0] != "W":
            continue
        try:
            _, wid, x, y, w, h, title = parts
            windows[json.loads(title)] = (int(x), int(y), int(w), int(h))
        except (ValueError, json.JSONDecodeError):
            continue
    return windows


def pick_point(target, other):
    """Escolhe um canto da janela alvo que NÃO caia dentro da outra janela."""
    x, y, w, h = target
    ox, oy, ow, oh = other
    for px, py in ((x + 10, y + 10), (x + w - 10, y + h - 10),
                   (x + 10, y + h - 10), (x + w - 10, y + 10)):
        if not (ox <= px <= ox + ow and oy <= py <= oy + oh):
            return px, py
    return x + 10, y + 10


def wait_log(path, needle, timeout=12):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if needle in path.read_text(errors="replace"):
            return True
        time.sleep(0.05)
    return False


def run_case(backend, sync):
    sock = f"wayland-aero-clip-{backend}-{os.getpid()}"
    label = "racy" if not sync else "sync"
    dir_ = OUT / f"{backend}-{label}"
    dir_.mkdir(parents=True, exist_ok=True)
    comp_log = dir_ / "comp.log"
    a_err = dir_ / "a.err"
    b_err = dir_ / "b.err"
    input_sock = str(dir_ / "input.sock")
    ipc_sock = str(dir_ / "ipc.sock")
    env = {**os.environ, "RUST_LOG": "debug", "AERO_TEST_INPUT_SOCK": input_sock,
           "AERO_WINDOW_IPC_SOCK": ipc_sock}
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET"):
        env.pop(key, None)
    args = [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", backend,
            "--socket", sock]
    if backend == "winit":
        args += ["--size", "1280x800"]
    comp = subprocess.Popen(args, cwd=ROOT, stdout=comp_log.open("wb"),
                            stderr=subprocess.STDOUT, env=env)
    a = b = None
    try:
        deadline = time.time() + 15
        while time.time() < deadline and not (RUNTIME / sock).exists():
            if comp.poll() is not None:
                print("FALHA: compositor saiu antes do socket")
                return False
            time.sleep(0.05)
        if not (RUNTIME / sock).exists():
            print("FALHA: socket do compositor não apareceu")
            return False
        time.sleep(0.2)
        child_env = {**os.environ, "WAYLAND_DISPLAY": sock, "GDK_BACKEND": "wayland"}
        a = subprocess.Popen([str(ROOT / "target/debug/examples/clipboard_client"), "--offer"],
                             cwd=ROOT, stdout=subprocess.DEVNULL, stderr=a_err.open("wb"), env=child_env)
        b = subprocess.Popen([str(ROOT / "target/debug/examples/clipboard_client"), "--receive"],
                             cwd=ROOT, stdout=subprocess.DEVNULL, stderr=b_err.open("wb"), env=child_env)
        if not wait_log(a_err, "CLIP_A ready") or not wait_log(b_err, "CLIP_B ready"):
            print("FALHA: clientes não ficaram prontos")
            return False
        deadline = time.time() + 10
        windows = {}
        while time.time() < deadline and len(windows) < 2:
            if pathlib.Path(ipc_sock).exists():
                windows = query_windows(ipc_sock)
            time.sleep(0.1)
        if "aero-clipboard-A" not in windows or "aero-clipboard-B" not in windows:
            print(f"FALHA: IPC não listou as duas janelas: {windows}")
            return False
        ax, ay, aw, ah = windows["aero-clipboard-A"]
        bx, by, bw, bh = windows["aero-clipboard-B"]
        pt_a = pick_point((ax, ay, aw, ah), (bx, by, bw, bh))
        pt_b = pick_point((bx, by, bw, bh), (ax, ay, aw, ah))
        print(f"janelas IPC={windows} pt_a={pt_a} pt_b={pt_b}")
        # Foco A: o serial do botão autoriza set_selection.
        send_input(input_sock, f"motion {pt_a[0]} {pt_a[1]}")
        send_input(input_sock, "button press")
        send_input(input_sock, "button release")
        if sync:
            if not wait_log(a_err, "CLIP_A SELECTION_SET"):
                print("FALHA: A nunca definiu a seleção")
                return False
            time.sleep(0.3)
        else:
            # Baseline racy: foco B SEM esperar a seleção de A ser processada.
            pass
        # Foco B: o Smithay envia data_offer + selection para o novo foco.
        send_input(input_sock, f"motion {pt_b[0]} {pt_b[1]}")
        send_input(input_sock, "button press")
        send_input(input_sock, "button release")
        ok = wait_log(b_err, "CLIP_B OK", timeout=12)
        if b.poll() is None:
            b.terminate()
            b.wait(timeout=5)
        a_out = a_err.read_text(errors="replace")
        b_out = b_err.read_text(errors="replace")
        log = comp_log.read_text(errors="replace")
        print(f"=== {backend}/{label}: cliente A (cru) ===")
        print(a_out, end="")
        print(f"=== {backend}/{label}: cliente B (cru) ===")
        print(b_out, end="")
        print(f"=== {backend}/{label}: foco/erros do compositor ===")
        for line in log.splitlines():
            if "foco de teclado alterado" in line or "ProtocolError" in line or "protocol error" in line:
                print(line)
        checks = {
            "A ofereceu e definiu seleção": "CLIP_A SELECTION_SET" in a_out and "CLIP_A SEND" in a_out,
            f"B recebeu exatamente o texto": f"CLIP_B GOT {TEXT}" in b_out,
            "B terminou OK": "CLIP_B OK" in b_out,
            "zero erros de protocolo": "ProtocolError" not in log and "protocol error" not in log,
        }
        for name, passed in checks.items():
            print(f"{'OK ' if passed else 'FALHA '} {name}")
        return all(checks.values()) and ok
    finally:
        for proc in (b, a):
            if proc is not None and proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=5)
        if comp.poll() is None:
            comp.terminate()
            comp.wait(timeout=5)


def main():
    sync = "--no-sync" not in sys.argv
    results = {}
    for backend in ("headless", "winit"):
        results[backend] = run_case(backend, sync)
    print("VEREDITO: " + ("OK" if all(results.values()) else "FALHA"))
    print(f"resultados={results}")
    return 0 if all(results.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
