#!/usr/bin/env python3
"""F2-surface-host-frame: o frame de produção vem do host genérico.

Baseline antes da migração: examples/frame_webview.rs ainda contém o spike
(independente do host), então a checagem estrutural FALHA. Depois: o mesmo
arquivo vira um shim fino e frame_buttons.py passa.
"""
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
SHIM = ROOT / "crates/aeroshell-shell/examples/frame_webview.rs"


def main():
    text = SHIM.read_text()
    shim_ok = (
        len(text.splitlines()) <= 30
        and re.search(r"aeroshell_shell::surfaces::frame::main\s*\(\s*\)", text) is not None
    )
    print("=== shim do frame (cru) ===")
    print(text[:1000])
    print(f"shim_ok={shim_ok} linhas={len(text.splitlines())}")
    if not shim_ok:
        print("VEREDITO: FALHA")
        return 1

    print("=== frame_buttons.py contra o host ===")
    result = subprocess.run(
        ["python3", "tests/integration/frame_buttons.py"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=180,
    )
    print(result.stdout[-2500:])
    print(result.stderr[-1000:])
    ok = result.returncode == 0 and "VEREDITO: OK" in result.stdout
    print("VEREDITO: OK" if ok else "VEREDITO: FALHA")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
