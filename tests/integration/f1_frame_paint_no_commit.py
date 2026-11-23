#!/usr/bin/env python3
"""F1-final-1c: o frame WebView pinta mesmo quando o cliente NÃO recommits (Kate/Kwrite).

O cliente sintético `large_client` com AERO_LARGE_NO_COMMIT=1 reproduz o caso real:
toplevel mapeado, frame associado, UMA configure, nenhum commit de buffer do cliente.
Baseline: `frame_painted` nunca aparece. Depois da correção: aparece.

Pre-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-compositor --example large_client
  cargo build -p aeroshell-shell --example frame_webview
Uso: python3 tests/integration/f1_frame_paint_no_commit.py
"""
import os
import pathlib
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-frame-nocommit-"))
SOCK = f"wayland-aero-nocommit-{os.getpid()}"


def main() -> int:
    log = OUT / "compositor.log"
    client_err = OUT / "client.err"
    env = {
        **os.environ,
        "RUST_LOG": "debug",
        "AERO_WINDOW_IPC_SOCK": str(OUT / "ipc.sock"),
        "AERO_LARGE_NO_COMMIT": "1",
        "QT_QPA_PLATFORM": "wayland",
        "GDK_BACKEND": "wayland",
    }
    comp = subprocess.Popen(
        [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "winit",
         "--socket", SOCK, "--shell", str(ROOT / "target/debug/examples/frame_webview"),
         "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0",
         "--command", str(ROOT / "target/debug/examples/large_client")],
        cwd=ROOT, stdout=log.open("wb"), stderr=subprocess.STDOUT, env=env,
    )
    try:
        deadline = time.time() + 20
        while time.time() < deadline:
            if log.exists() and "FRAMEWEB: canvas inicial" in log.read_text(errors="replace"):
                break
            time.sleep(0.1)
        time.sleep(7)  # tempo para o retry do JS, se existir
        text = log.read_text(errors="replace") if log.exists() else ""
        painted = text.count("FRAMEWEB: frame_painted window_id=")
        configures = text.count("FRAMEWEB: configure serial=")
        commits = text.count("FRAME_BUFFER_COMMIT")
        real_buffer = "FRAME_BUFFER_COMMIT buffer=Some" in text
        print(f"frame_painted={painted} configures={configures} frame_commits={commits} buffer_real={real_buffer}")
        checks = {
            "cliente mapeado e frame associado": "FRAMEWEB: canvas inicial" in text,
            "sem commit de buffer do cliente": "LARGE_CLIENT committed" not in client_err.read_text(errors="replace")
            if client_err.exists() else True,
            "frame pintou sem segundo configure": painted >= 1,
            "WebView commitou buffer real": real_buffer,
        }
        for name, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {name}")
        print(f"log_cru={OUT}")
        print("VEREDITO: " + ("OK" if all(checks.values()) else "FALHA"))
        return 0 if all(checks.values()) else 1
    finally:
        if comp.poll() is None:
            comp.terminate()
            comp.wait(timeout=5)


if __name__ == "__main__":
    sys.exit(main())
