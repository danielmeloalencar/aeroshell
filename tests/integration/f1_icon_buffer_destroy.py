#!/usr/bin/env python3
"""F1-toplevel-icon-protocol: regression sintética do bug do hook de destruição.

O cliente `toplevel_icon_client` com AERO_ICON_DESTROY_AFTER_SET=1 faz:
    create_buffer -> add_buffer -> set_icon -> icon.destroy() -> wl_buffer.destroy()
que é a sequência exata do Qt (kwrite) measure em `/tmp/f1-icon-wldbg-qj1b27v8`.

Antes da correção, o Smithay 0.7.0 registra um hook de destruição no wl_buffer
(`xdg_toplevel_icon.rs:360-369`) que NÃO é desregistrado quando o ícone morre
(`remove_destruction_hook` usa `hook.id != hook_id` em `shm/mod.rs:496`) e posta
`xdg_toplevel_icon_v1.no_buffer`, matando o cliente.

Critério: nenhum `CLIENT_DISCONNECT ... ProtocolError` e nenhum `wl_display.error`
no log; o cliente imprime `ICON_CLIENT DESTROY_AFTER_SET_OK`.

Uso: python3 tests/integration/f1_icon_buffer_destroy.py
Pre-requisito: cargo build -p aeroshell-compositor --features bench
              cargo build -p aeroshell-compositor --example toplevel_icon_client
"""
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f1-icon-destroy-"))
SOCK = f"wayland-aero-icon-destroy-{os.getpid()}"
LOG = OUT / "compositor.log"
CLIENT = OUT / "client.err"


def main() -> int:
    env = {
        **os.environ,
        "RUST_LOG": "debug",
        "AERO_WINDOW_IPC_SOCK": str(OUT / "ipc.sock"),
        "AERO_ICON_DESTROY_AFTER_SET": "1",
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
         "--command", str(ROOT / "target/debug/examples/toplevel_icon_client")],
        cwd=ROOT, env=env, stdout=LOG.open("wb"), stderr=subprocess.STDOUT,
        start_new_session=True,
    )
    try:
        deadline = time.time() + 25
        while time.time() < deadline:
            seen = LOG.read_text(errors="replace") + (CLIENT.read_text(errors="replace") if CLIENT.exists() else "")
            if "ICON_CLIENT DESTROY_AFTER_SET_OK" in seen:
                break
            time.sleep(0.1)
        time.sleep(1)
        text = LOG.read_text(errors="replace") if LOG.exists() else ""
        client_text = CLIENT.read_text(errors="replace") if CLIENT.exists() else ""
        # O supervisor redireciona o stdout do --command para o log do compositor;
        # o marcador pode estar lá (client.err fica vazio nesse caminho).
        combined = text + "\n" + client_text

        protocol_errors = [l.strip() for l in text.splitlines()
                           if "CLIENT_DISCONNECT" in l and "ProtocolError" in l]
        wl_errors = [l.strip() for l in text.splitlines() if "wl_display@1.error" in l]
        client_ok = "ICON_CLIENT DESTROY_AFTER_SET_OK" in combined

        print(f"protocol_errors={len(protocol_errors)} wl_display_errors={len(wl_errors)} "
              f"client_ok={client_ok}")
        print("--- CLIENT_DISCONNECT ProtocolError (cru) ---")
        for line in protocol_errors[:5]:
            print(line[:300])
        print("--- wl_display.error (cru) ---")
        for line in wl_errors[:5]:
            print(line[:300])
        print("--- client.err (cru) ---")
        print(client_text[-800:])
        checks = {
            "nenhum ProtocolError": len(protocol_errors) == 0,
            "nenhum wl_display.error": len(wl_errors) == 0,
            "cliente completou destroy-after-set": client_ok,
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
