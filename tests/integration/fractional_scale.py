#!/usr/bin/env python3
"""F1f: prova por pixels de escala fracionaria sem gap borda-do-frame <-> cliente.

Roda o headless com `--scale S`, captura um dump full-res (AERO_PIXEL_FULL, sobe
bench) e verifica:
  * FSCALE_SURFACE com a escala esperada (so `wp_fractional_scale_v1` implementado);
  * espessura da borda e altura da titlebar acompanham S sem drift;
  * nao ha pixels de CLEAR entre a borda do frame e o conteudo do cliente
    (continuidade: mesma escala do WaylandSurfaceRenderElement), em 1.0/1.25/1.5.

Pre-requisito: cargo build -p aeroshell-compositor --features bench && cargo build --workspace --examples
"""
import math
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
BIN = ROOT / "target/debug/aeroshell-compositor"
SHELL = ROOT / "target/debug/examples/frame_webview"
CLEAR = (20, 20, 25)
FRAME = (43, 43, 43)          # titlebar/borda do frame (medido em 02p/F1c)
CLIENT = (36, 36, 36)         # fundo do foot (medido em F1d3)


def near(px, ref, tol=3):
    return all(abs(px[i] - ref[i]) <= tol for i in range(3))


def read_dump(path, timeout=25.0):
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        if path.exists() and path.stat().st_size >= 1280 * 800 * 4:
            data = path.read_bytes()
            last = data
            # O primeiro dump é todo clear; esperar até haver conteúdo real.
            if any(data[i : i + 3] != b"\x19\x14\x14" for i in range(0, len(data), 4 * 997)):
                return data
        time.sleep(0.1)
    return last


def analyze(data, scale):
    """Mede borda/titlebar pela distancia entre os bboxes de frame e cliente."""
    w, h = 1280, 800

    def rgb(x, y):
        i = (y * w + x) * 4
        return (data[i + 2], data[i + 1], data[i])

    fb = cb = None
    for y in range(0, h, 2):
        for x in range(0, w, 2):
            p = rgb(x, y)
            if near(p, FRAME):
                fb = (x, y, x, y) if fb is None else (min(fb[0], x), min(fb[1], y), max(fb[2], x), max(fb[3], y))
            elif near(p, CLIENT):
                cb = (x, y, x, y) if cb is None else (min(cb[0], x), min(cb[1], y), max(cb[2], x), max(cb[3], y))
    if fb is None or cb is None:
        return 0, 0, False
    # O output headless usa Transform::Flipped180: a titlebar fica na BORDA
    # INFERIOR do dump (bbox do frame termina abaixo do bbox do cliente).
    border = cb[0] - fb[0]
    titlebar = fb[3] - cb[3]
    # Continuidade: entre o bbox do frame e o do cliente NAO pode haver CLEAR.
    ymid = (cb[1] + cb[3]) // 2
    xmid = (cb[0] + cb[2]) // 2
    gap_h = any(near(rgb(x, ymid), CLEAR) for x in range(fb[0], cb[0] + 1))
    gap_v = any(near(rgb(xmid, y), CLEAR) for y in range(cb[3], fb[3] + 1))
    return border, titlebar, not (gap_h or gap_v)


def run_scale(scale):
    tag = str(scale).replace(".", "_")
    out = pathlib.Path(tempfile.mkdtemp(prefix=f"aero-fscale-{tag}-"))
    sock = f"wayland-aero-fscale-{tag}-{os.getpid()}"
    log = out / "compositor.log"
    env = {
        **os.environ,
        "AERO_PIXEL_DUMP_DIR": str(out),
        "AERO_PIXEL_FULL": "1",
        "RUST_LOG": "info",
    }
    comp = subprocess.Popen(
        [str(BIN), "--backend", "headless", "--scale", str(scale), "--socket", sock,
         "--shell", str(SHELL), "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0",
         "--command", "foot"],
        cwd=ROOT, stdout=log.open("wb"), stderr=subprocess.STDOUT, env=env,
    )
    try:
        data = read_dump(out / "full_000000.bgra")
        text = log.read_text(errors="replace") if log.exists() else ""
        if data is None:
            return {"dump": False, "log": str(log)}, text
        border, titlebar, continuous = analyze(data, scale)
        expected_border = 6 * scale
        expected_titlebar = 30 * scale
        checks = {
            "dump full-res": True,
            # Em 1.0 o cliente nao precisa bindingar o protocolo: a ausencia do
            # log é o comportamento esperado, nao falha (regressao 1.0 intacta).
            "FSCALE_SURFACE com escala": (
                scale == 1.0
                or bool(
                    re.search(rf"FSCALE_SURFACE .* scale={re.escape(str(scale))}(?: |$)", text)
                )
            ),
            "borda acompanha escala": abs(border - expected_border) <= 1.0,
            "titlebar acompanha escala": abs(titlebar - expected_titlebar) <= 1.5,
            "borda COLADA ao cliente (sem gap)": continuous,
        }
        print(f"-- scale {scale}: border={border} (esperado~{expected_border:.1f}) "
              f"titlebar={titlebar} (esperado~{expected_titlebar:.1f}) continuo={continuous}")
        return checks, text
    finally:
        if comp.poll() is None:
            comp.terminate()
            comp.wait(timeout=5)


def main():
    results = {}
    for scale in (1.0, 1.25, 1.5):
        checks, text = run_scale(scale)
        results[scale] = checks
        for name, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} scale={scale} {name}")
        if not checks.get("dump"):
            print(f"--- log cru ({scale}) ---\n{text[-2000:]}")
    ok = all(all(v.values()) for v in results.values())
    print("VEREDITO: " + ("OK" if ok else "FALHA"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
