#!/usr/bin/env python3
"""F1-final-1c (Passo 1): configure inicial do canvas na frame layer-surface.

O cliente sintetico `large_client` com AERO_LARGE_NO_COMMIT=1 mapeia o toplevel
mas nunca commita buffer de conteudo. Antes do fix, `new_layer_surface`
(layer_shell.rs:63-75) so envia `zwlr_layer_surface_v1.configure` quando
`client_content_size(window_id)` existe -> sem configure/ack, o GTK nunca anexa
wl_buffer e o frame nunca pinta.

Criterio (tudo no MESMO log cru, nada descartado):
  1. `zwlr_layer_surface_v1#*.configure` aparece (configure do CANVAS, nao o
     `aero_frame_v1.configure` que ja existia);
  2. `wl_surface#*.attach` aparece no cliente shell (WAYLAND_DEBUG=1);
  3. `FRAME_BUFFER_COMMIT buffer=Some`;
  4. `FRAMEWEB: frame_painted window_id=` >= 1;
  5. GUARDA 02q/ADR 0007: nenhum `FSCALE_FRAME ... window_id=0 ... frame_dst`
     com largura >=1000 antes do content existir (frame nunca aparece em
     tamanho de canvas cheio na origem do output).

Uso: python3 tests/integration/f1_frame_configure_initial.py
Pre-requisito: binaries compilados (check.sh / cargo build --features bench).
"""
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f1-layer-config-"))
SOCK = f"wayland-aero-f1-layer-{os.getpid()}"
LOG = OUT / "compositor.log"


def main() -> int:
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
         "--shell-env", "WAYLAND_DEBUG=1",
         "--command", str(ROOT / "target/debug/examples/large_client")],
        cwd=ROOT, stdout=LOG.open("wb"), stderr=subprocess.STDOUT, env=env,
    )
    try:
        deadline = time.time() + 20
        while time.time() < deadline:
            if LOG.exists() and "FRAMEWEB: canvas inicial" in LOG.read_text(errors="replace"):
                break
            time.sleep(0.1)
        time.sleep(6)
        text = LOG.read_text(errors="replace") if LOG.exists() else ""

        layer_configures = re.findall(r"zwlr_layer_surface_v1#\d+\.configure", text)
        attaches = re.findall(r"wl_surface#\d+\.attach", text)
        painted = text.count("FRAMEWEB: frame_painted window_id=")
        real_buffer = "FRAME_BUFFER_COMMIT buffer=Some" in text

        # Guarda: todo frame_dst medido com o cliente sem conteudo deve ser pequeno.
        frame_dsts = []
        for m in re.finditer(r"FSCALE_FRAME [^\n]*window_id=0 [^\n]*frame_dst=Size<[^>]*> \{ w: (\d+), h: (\d+) \}", text):
            frame_dsts.append((int(m.group(1)), int(m.group(2))))
        full_canvas_render = any(w >= 1000 or h >= 1000 for (w, h) in frame_dsts)

        print(f"layer_configures={len(layer_configures)} attaches={len(attaches)} "
              f"frame_painted={painted} buffer_real={real_buffer} "
              f"frame_dsts={frame_dsts[:8]} full_canvas_render={full_canvas_render}")
        checks = {
            "configure do canvas na layer surface": len(layer_configures) >= 1,
            "wl_surface.attach no cliente shell": len(attaches) >= 1,
            "FRAME_BUFFER_COMMIT buffer=Some": real_buffer,
            "frame_painted": painted >= 1,
            "nenhum frame em canvas cheio antes do content": not full_canvas_render,
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
