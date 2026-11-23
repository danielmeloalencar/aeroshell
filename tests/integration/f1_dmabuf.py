#!/usr/bin/env python3
"""F1-dmabuf: global linux-dmabuf-v1, tranches reais e import de buffer DMA-BUF.

Baseline (sem o global): o cliente reporta o global AUSENTE e o harness da
VEREDITO: FALHA. Depois da correção: global presente, tranches com formatos do
renderer, `DMABUF_IMPORT` no log bench, pixels do cliente no dump full-res e
zero erros de protocolo.

Pre-requisito (o check.sh reconstrói o bin sem bench):
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-compositor --example dmabuf_client

Uso: python3 tests/integration/f1_dmabuf.py
"""
import os
import pathlib
import re
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNTIME = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
OUT = pathlib.Path("/tmp/opencode/f1-dmabuf/harness")
OUT.mkdir(parents=True, exist_ok=True)
SOCK = f"wayland-aero-dmabuf-{os.getpid()}"
COMP_LOG = OUT / f"comp-{os.getpid()}.log"
CLIENT_ERR = OUT / f"client-{os.getpid()}.err"


def count_color(data: bytes, color: tuple[int, int, int], tol: int = 12) -> int:
    cr, cg, cb = color
    n = 0
    for i in range(0, len(data) - 3, 4):
        b, g, r, a = data[i], data[i + 1], data[i + 2], data[i + 3]
        if a < 200:
            continue
        if abs(r - cr) <= tol and abs(g - cg) <= tol and abs(b - cb) <= tol:
            n += 1
    return n


def main() -> int:
    env = {
        **os.environ,
        "RUST_LOG": "debug",
        "AERO_PIXEL_DUMP_DIR": str(OUT),
        "AERO_PIXEL_FULL": "1",
    }
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET"):
        env.pop(key, None)
    comp = subprocess.Popen(
        [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
         "--socket", SOCK],
        cwd=ROOT, stdout=COMP_LOG.open("wb"), stderr=subprocess.STDOUT, env=env,
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
            [str(ROOT / "target/debug/examples/dmabuf_client")],
            cwd=ROOT, stdout=subprocess.DEVNULL, stderr=CLIENT_ERR.open("wb"),
            env={**os.environ, "WAYLAND_DISPLAY": SOCK},
        )
        # O dump full é sobrescrito a cada frame; amostrar enquanto o cliente
        # vive. Nenhuma amostra é descartada — todas são impressas.
        dump = OUT / "full_000000.bgra"
        samples: list[tuple[int, int]] = []
        for i in range(30):
            time.sleep(0.1)
            if dump.exists():
                data = dump.read_bytes()
                m = re.search(r"DMABUF_CLIENT COLOR r=(\d+) g=(\d+) b=(\d+)",
                              CLIENT_ERR.read_text(errors="replace"))
                color = tuple(int(x) for x in m.groups()) if m else (220, 30, 30)
                samples.append((i, count_color(data, color)))

        deadline = time.time() + 10
        while time.time() < deadline and client.poll() is None:
            time.sleep(0.1)
        if client.poll() is None:
            client.terminate()
            client.wait(timeout=5)

        out = CLIENT_ERR.read_text(errors="replace")
        log = COMP_LOG.read_text(errors="replace")
        print("=== cliente (cru) ===")
        print(out, end="")
        print("=== compositor (linhas dmabuf) ===")
        for line in log.splitlines():
            if "DMABUF_" in line or "ProtocolError" in line or "protocol error" in line:
                print(line)
        for i, n in samples:
            print(f"amostra i={i:02d} pix={n}")
        _, pixels = max(samples, key=lambda s: s[1]) if samples else (0, 0)
        formats = re.findall(r"DMABUF_CLIENT FORMAT fourcc=0x([0-9a-f]+) modifier=0x([0-9a-f]+)", out)
        checks = {
            "global zwp_linux_dmabuf_v1 no registry": "DMABUF_CLIENT GLOBAL version=" in out,
            "tranches com formatos/modifiers": len(formats) > 0,
            "buffer DMA-BUF criado pelo cliente": "DMABUF_CLIENT BUFFER " in out,
            "handler do compositor recebeu o import": "DMABUF_IMPORT " in log,
            "pixels do cliente no dump": pixels >= 20,
            "cliente terminou OK": "DMABUF_CLIENT OK" in out,
            "zero erros de protocolo": (
                "ProtocolError" not in log and "protocol error" not in log
            ),
        }
        print(f"amostras={len(samples)} pix={pixels} formatos={len(formats)}")
        for name, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {name}")
        verdict = "OK" if all(checks.values()) else "FALHA"
        print("VEREDITO: " + verdict)
        return 0 if verdict == "OK" else 1
    finally:
        if client is not None and client.poll() is None:
            client.terminate()
            client.wait(timeout=5)
        if comp.poll() is None:
            comp.terminate()
            comp.wait(timeout=5)


if __name__ == "__main__":
    sys.exit(main())
