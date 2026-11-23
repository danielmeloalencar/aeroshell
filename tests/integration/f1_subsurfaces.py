#!/usr/bin/env python3
"""F1-subsurfaces: wl_subsurface real (sync + desync), hit-test e pixels.

Pré-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-compositor --example subsurface_client
  cargo build -p aeroshell-shell --example frame_webview

Roda no headless, sem display; fecha somente os PIDs que iniciou.
Uso: python3 tests/integration/f1_subsurfaces.py
"""
import os
import pathlib
import re
import socket
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-subsurface-"))
SOCK = f"wayland-aero-subsurface-{os.getpid()}"
RUNTIME = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
# Janela cliente em (60,80): filho sync em (20,20) -> (80,100); desync em (220,20) -> (280,100).
POINTS = [(80, 100), (280, 100)]


def send_input(path: str, line: str):
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.connect(path)
            s.sendall(f"{line}\n".encode())
    except OSError:
        pass


def count_colors(data: bytes, colors, tol=6):
    """Conta pixels BGRA próximos de cada cor (r,g,b)."""
    counts = [0] * len(colors)
    for i in range(0, len(data) - 3, 4):
        b, g, r, a = data[i], data[i + 1], data[i + 2], data[i + 3]
        if a < 200:
            continue
        for idx, (cr, cg, cb) in enumerate(colors):
            if abs(r - cr) <= tol and abs(g - cg) <= tol and abs(b - cb) <= tol:
                counts[idx] += 1
                break
    return counts


def main() -> int:
    comp_log = OUT / "compositor.log"
    client_err = OUT / "client.err"
    input_sock = str(OUT / "input.sock")
    env = {
        **os.environ,
        "RUST_LOG": "debug",
        "AERO_TEST_INPUT_SOCK": input_sock,
        "AERO_PIXEL_DUMP_DIR": str(OUT),
        "AERO_PIXEL_FULL": "1",
    }
    comp = subprocess.Popen(
        [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
         "--socket", SOCK, "--shell", str(ROOT / "target/debug/examples/frame_webview"),
         "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
        cwd=ROOT, stdout=comp_log.open("wb"), stderr=subprocess.STDOUT, env=env,
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
            [str(ROOT / "target/debug/examples/subsurface_client")],
            cwd=ROOT, stdout=subprocess.DEVNULL, stderr=client_err.open("wb"),
            env={**os.environ, "WAYLAND_DISPLAY": SOCK},
        )
        # Espera os filhos existirem antes de injetar input.
        deadline = time.time() + 15
        while time.time() < deadline and client.poll() is None:
            if "SUBSURFACE_CLIENT parent_committed" in client_err.read_text(errors="replace"):
                break
            time.sleep(0.05)
        # Motions + cliques repetidos: mantêm o headless desenhando e exercitam o hit-test.
        # Amostra o dump a CADA iteração: o arquivo full é sobrescrito por frame, então
        # ler só no fim capturaria um frame depois do cliente ter saído (amostra ruim).
        colors = [(220, 30, 30), (30, 190, 40), (30, 60, 220)]  # sync, desync, pai
        dump = OUT / "full_000000.bgra"
        samples = []
        for i in range(30):
            x, y = POINTS[i % 2]
            send_input(input_sock, f"motion {x} {y}")
            send_input(input_sock, "button press")
            send_input(input_sock, "button release")
            time.sleep(0.1)
            if dump.exists():
                samples.append((i, count_colors(dump.read_bytes(), colors)))
        deadline = time.time() + 15
        while time.time() < deadline and client.poll() is None:
            time.sleep(0.1)
        if client.poll() is None:
            client.terminate()
            client.wait(timeout=5)

        out = client_err.read_text(errors="replace")
        log = comp_log.read_text(errors="replace")
        print("=== cliente ===")
        print(out, end="")
        # Nenhuma amostra é descartada: todas são impressas; a melhor (mais pixels
        # de cliente) é usada como evidência, com a justificativa explícita.
        for i, counts in samples:
            print(f"amostra i={i:02d} sync={counts[0]} desync={counts[1]} pai={counts[2]}")
        _, counts = max(samples, key=lambda s: sum(s[1])) if samples else (0, [0, 0, 0])
        size_m = re.search(r"PIXEL_FRAME_FULL size=(\d+)x(\d+)", log)
        print(f"amostras={len(samples)} dump={dump.exists()} "
              f"size={size_m.group(0) if size_m else '?'} "
              f"pix_sync={counts[0]} pix_desync={counts[1]} pix_pai={counts[2]}")

        checks = {
            "cliente criou os 2 filhos": out.count("SUBSURFACE_CLIENT child_created") == 2,
            "cliente comitou o pai": "SUBSURFACE_CLIENT parent_committed" in out,
            "cliente terminou OK": "SUBSURFACE_CLIENT OK" in out,
            "hit-test no filho synced": "SUBSURFACE_CLIENT pointer kind=button label=synced" in out,
            "hit-test no filho desynced": "SUBSURFACE_CLIENT pointer kind=button label=desynced" in out,
            "sonda SUBSURFACE_COMMIT (bench)": "SUBSURFACE_COMMIT t=" in log,
            "sonda registrou sync e desync": (
                bool(re.search(r"SUBSURFACE_COMMIT .* sync=true", log))
                and bool(re.search(r"SUBSURFACE_COMMIT .* sync=false", log))
            ),
            "pixels do filho synced no dump": counts[0] >= 20,
            "pixels do filho desynced no dump": counts[1] >= 20,
            "pixels do pai no dump": counts[2] >= 20,
            "sem erro de protocolo": not re.search(r"error \d+:|protocol error", log, re.I),
        }
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
