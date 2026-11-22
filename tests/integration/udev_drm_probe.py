#!/usr/bin/env python3
"""F1d2 — sonda GBM em renderD128 através do compositor.

O compositor expõe `--probe-gbm[=PATH]`: abre o nó DRM, cria um `GbmDevice`,
enumera formats/modifiers suportados, aloca um buffer object e um buffer via
`GbmAllocator` (o caminho que F1d3 usa), e imprime um JSON no stdout.

Este harness foi escrito ANTES da implementação (teste primeiro): contra o
binário atual, sem a flag, o compositor sai com exit 2 e o harness FALHA.
Nenhum VT é trocado, nenhum_master é adquirido, nenhum shell é lançado.
"""
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BIN = os.path.join(ROOT, "target", "debug", "aeroshell-compositor")
NODE = os.environ.get("AERO_DRM_RENDER_NODE", "/dev/dri/renderD128")


def fail(msg: str) -> None:
    print(f"FALHA: {msg}", file=sys.stderr)
    sys.exit(1)


def main() -> None:
    if not os.path.isfile(BIN):
        fail(f"binário ausente: {BIN} (rode `cargo build --workspace` primeiro)")
    if not os.path.exists(NODE):
        fail(f"nó DRM ausente: {NODE}")

    cmd = [BIN, f"--probe-gbm={NODE}"]
    print("$ " + " ".join(cmd))
    try:
        proc = subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=60,
        )
    except subprocess.TimeoutExpired:
        fail("compositor excedeu 60 s (sonda travou?)")

    print(f"exit = {proc.returncode}")
    if proc.stderr.strip():
        print("--- stderr ---")
        print(proc.stderr.strip(), file=sys.stderr)
    if proc.returncode != 0:
        fail(
            f"compositor saiu {proc.returncode}; sonda GBM não implementada ou falhou"
        )

    try:
        report = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        print("--- stdout ---", file=sys.stderr)
        print(proc.stdout, file=sys.stderr)
        fail(f"stdout não é JSON: {exc}")

    if report.get("ok") is not True:
        fail(f"relatório marcado ok={report.get('ok')!r}")
    if not report.get("backend_name"):
        fail("backend_name vazio")
    if report.get("node") != NODE:
        fail(f"node divergente: {report.get('node')!r} != {NODE!r}")

    formats = report.get("formats", [])
    if not isinstance(formats, list) or not formats:
        fail("nenhum format listado")
    renderable = [
        f for f in formats if f.get("render") and f.get("scanout")
    ]
    if not renderable:
        fail("nenhum format com RENDERING|SCANOUT")

    alloc = report.get("allocation") or {}
    if not alloc.get("dmabuf"):
        fail("alocação direta (gbm_bo_create) não entregou dmabuf")
    if not isinstance(alloc.get("stride"), int) or alloc["stride"] <= 0:
        fail(f"stride inválido: {alloc.get('stride')!r}")

    via_allocator = report.get("allocator") or {}
    if not via_allocator.get("dmabuf"):
        fail("GbmAllocator::create_buffer_with_flags não entregou dmabuf")

    print("--- relatório da sonda ---")
    print(json.dumps(report, indent=2, ensure_ascii=False))
    print("--- resumo ---")
    print(f"nó={NODE} backend={report['backend_name']}")
    print(f"formats RENDERING|SCANOUT: {[f['fourcc'] for f in renderable]}")
    print(
        "alocação: "
        f"{alloc['fourcc']} {alloc['width']}x{alloc['height']} "
        f"stride={alloc['stride']} modifier={alloc.get('modifier')} dmabuf=sim"
    )
    print(
        "GbmAllocator: "
        f"{via_allocator['fourcc']} {via_allocator['width']}x{via_allocator['height']} "
        f"modifier={via_allocator.get('modifier')} dmabuf=sim"
    )
    # Modos são informativos: VM pode não ter conector ativo.
    modes = report.get("drm_modes") or {}
    if "error" in modes:
        print(f"DRM modes: indisponível ({modes['error']})")
    else:
        connectors = modes.get("connectors", [])
        print(f"DRM modes: {modes.get('node')} conectores={len(connectors)}")
        for conn in connectors:
            mode_strings = [
                f"{m['width']}x{m['height']}@{m['vrefresh_hz']}" for m in conn["modes"]
            ]
            print(f"  {conn['connector']} state={conn['state']} modes={mode_strings}")
    print("OK: sonda GBM em renderD128 passou")


if __name__ == "__main__":
    main()
