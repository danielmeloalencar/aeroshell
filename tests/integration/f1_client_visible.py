#!/usr/bin/env python3
"""F1-final-1c: medicao que faltava — cliente VIVO e conteudo na tela.

Os harnesses anteriores (f1_frame_configure_initial.py) mediam apenas
`frame_painted` + `FRAME_BUFFER_COMMIT buffer=Some`. Isso PASSOU enquanto o
humano viu "uma janela decorada com titulo Janela, sem conteudo": o frame WebView
pintava DEPOIS de o cliente Qt ser desconectado por ProtocolError em
xdg_toplevel_icon_v1. O titulo "janela" e o placeholder do HTML
(frame_webview.rs:864), nao um titulo real.

Este harness mede o que o humano viu:
  1. nenhum `CLIENT_DISCONNECT ... ProtocolError` no cliente principal;
  2. IPC `list` nao vazio (janela cliente registrada);
  3. `FRAMEWEB: title_applied` com titulo diferente de `janela`.

Uso: python3 tests/integration/f1_client_visible.py
Pre-requisito: cargo build -p aeroshell-compositor --features bench
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
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f1-client-visible-"))
SOCK = f"wayland-aero-f1-client-{os.getpid()}"
LOG = OUT / "compositor.log"


def ipc_list(path: pathlib.Path) -> str:
    try:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.connect(str(path))
        s.sendall(b"list\n")
        time.sleep(0.3)
        data = s.recv(4096).decode(errors="replace")
        s.close()
        return data
    except OSError as err:
        return f"ERRO: {err}"


def main() -> int:
    env = {
        **os.environ,
        "RUST_LOG": "debug",
        "AERO_WINDOW_IPC_SOCK": str(OUT / "ipc.sock"),
        "QT_QPA_PLATFORM": "wayland",
        "GDK_BACKEND": "wayland",
    }
    for stale in (f"/run/user/1000/{SOCK}", str(OUT / "ipc.sock")):
        try:
            os.unlink(stale)
        except FileNotFoundError:
            pass
    comp = subprocess.Popen(
        [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "winit",
         "--socket", SOCK, "--shell", str(ROOT / "target/debug/examples/frame_webview"),
         "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0",
         "--command", "kate --startanon"],
        cwd=ROOT, env=env, stdout=LOG.open("wb"), stderr=subprocess.STDOUT,
        start_new_session=True,
    )
    try:
        deadline = time.time() + 20
        while time.time() < deadline:
            if LOG.exists() and "FRAMEWEB: frame_painted" in LOG.read_text(errors="replace"):
                break
            time.sleep(0.1)
        time.sleep(8)  # tempo para o cliente Qt ser (ou nao) desconectado
        text = LOG.read_text(errors="replace") if LOG.exists() else ""

        errors = [l.strip() for l in text.splitlines()
                  if "CLIENT_DISCONNECT" in l and "ProtocolError" in l]
        titles = [l.strip() for l in text.splitlines() if "FRAMEWEB: title_applied" in l]
        painted = text.count("FRAMEWEB: frame_painted window_id=")
        real_titles = [l for l in titles if "titulo=janela" not in l]
        listing = ipc_list(OUT / "ipc.sock")
        has_client = listing.strip() not in ("", ".", "ERRO") and "." in listing

        print(f"frame_painted={painted} protocol_errors={len(errors)} "
              f"title_applied={len(titles)} titulos_reais={len(real_titles)}")
        print("--- CLIENT_DISCONNECT ProtocolError (cru) ---")
        for line in errors[:10]:
            print(line[:300])
        print("--- title_applied (cru) ---")
        for line in titles[:10]:
            print(line[:300])
        print(f"--- IPC list (cru): {listing!r} ---")
        checks = {
            "nenhum ProtocolError desconectou cliente": len(errors) == 0,
            "IPC list tem janela cliente": has_client,
            "titulo real (nao placeholder 'janela')": len(real_titles) >= 1,
        }
        for name, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {name}")
        print(f"log_cru={OUT}")
        print("VEREDITO: " + ("OK" if all(checks.values()) else "FALHA"))
        return 0 if all(checks.values()) else 1
    finally:
        if comp.poll() is None:
            comp.terminate()
            try:
                comp.wait(timeout=5)
            except subprocess.TimeoutExpired:
                comp.kill()
                comp.wait(timeout=5)


if __name__ == "__main__":
    sys.exit(main())
