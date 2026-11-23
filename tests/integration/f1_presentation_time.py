#!/usr/bin/env python3
"""F1-presentation-time: cliente wp_presentation, seq monotônico e timestamp real.

Pre-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-compositor --example presentation_client

Uso: python3 tests/integration/f1_presentation_time.py
"""
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNTIME = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-presentation-"))


def run_case(backend):
    sock = f"wayland-aero-presentation-{backend}-{os.getpid()}"
    dir_ = OUT / backend
    dir_.mkdir(parents=True, exist_ok=True)
    comp_log = dir_ / "comp.log"
    client_err = dir_ / "client.err"
    env = {**os.environ, "RUST_LOG": "debug"}
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET"):
        env.pop(key, None)
    args = [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", backend,
            "--socket", sock]
    if backend == "winit":
        args += ["--size", "1280x800"]
    comp = subprocess.Popen(args, cwd=ROOT, stdout=comp_log.open("wb"),
                            stderr=subprocess.STDOUT, env=env)
    client = None
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
        client = subprocess.Popen(
            [str(ROOT / "target/debug/examples/presentation_client")],
            cwd=ROOT, stdout=subprocess.DEVNULL, stderr=client_err.open("wb"),
            env={**os.environ, "WAYLAND_DISPLAY": sock},
        )
        deadline = time.time() + 25
        while time.time() < deadline and client.poll() is None:
            time.sleep(0.1)
        if client.poll() is None:
            client.terminate()
            client.wait(timeout=5)

        out = client_err.read_text(errors="replace")
        log = comp_log.read_text(errors="replace")
        print(f"=== {backend}: cliente (cru) ===")
        print(out, end="")
        print(f"=== {backend}: sondas do compositor ===")
        for line in log.splitlines():
            if "PRESENT_FEEDBACK" in line or "ProtocolError" in line:
                print(line)

        present = re.findall(
            r"PRES_CLIENT FEEDBACK seq=(\d+) tv=(\d+)\.(\d+) refresh=(\d+)", out)
        seqs = [int(s) for s, _, _, _ in present]
        probes = re.findall(r"PRESENT_FEEDBACK [^\n]*output=([^\s]+) seq=(\d+)", log)
        checks = {
            "global wp_presentation com clock monotonic": "PRES_CLIENT GLOBAL clock=1" in out,
            "pelo menos 2 presented": len(present) >= 2,
            "seq crescente": len(seqs) >= 2 and all(b > a for a, b in zip(seqs, seqs[1:])),
            "tv não-nulo": len(present) > 0 and all(
                int(sec) > 0 or int(nsec) > 0 for _, sec, nsec, _ in present),
            "sonda bench presente": len(probes) >= 1,
            "seq monotônico por output no compositor": (
                len(probes) >= 2 and all(int(b) > int(a) for (_, a), (_, b) in zip(probes, probes[1:]))
            ),
            "cliente terminou OK": "PRES_CLIENT OK" in out,
            "zero erros de protocolo": "ProtocolError" not in log and "protocol error" not in log,
        }
        print(f"amostras={len(present)} seqs={seqs} sondas={len(probes)}")
        for name, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {name}")
        return all(checks.values())
    finally:
        if client is not None and client.poll() is None:
            client.terminate()
            client.wait(timeout=5)
        if comp.poll() is None:
            comp.terminate()
            comp.wait(timeout=5)


def main():
    results = {backend: run_case(backend) for backend in ("headless", "winit")}
    print("resultados=" + str(results))
    print("VEREDITO: " + ("OK" if all(results.values()) else "FALHA"))
    return 0 if all(results.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
