#!/usr/bin/env python3
"""04b (pré-filtro): painel React lista janelas e o clique foca a clicada.

Roda o nested com 2 e depois 3 `foot`, lê a lista que o painel publica no log
via bridge e injeta um clique na 1ª linha (a janela de trás) usando o socket
de teste do compositor. A ordem de render (frente->tras) no log prova que
somente ela subiu.

A validação visual final é do humano, com a aplicação aberta (AGENTS.md §0.9).
"""

import atexit
import json
import os
import re
import socket
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
INPUT = Path(f"/tmp/opencode/aeroshell-react-input-{os.getpid()}.sock")
IPC = Path(f"/tmp/opencode/aeroshell-react-ipc-{os.getpid()}.sock")
COMP = ROOT / "target/debug/aeroshell-compositor"
BUNDLE = ROOT / "packages/panel/dist/app.js"
ANSI = re.compile(r"\x1b\[[0-9;]*m")
LOG = Path(f"/tmp/opencode/aeroshell-react-{os.getpid()}.log")
SOCK = f"wayland-aero-react-{os.getpid()}"


def clean():
    return ANSI.sub("", LOG.read_text(errors="ignore")) if LOG.exists() else ""


def fail(message):
    print(f"FALHA: {message}\n--- log do compositor ---\n{clean()}", file=sys.stderr)
    sys.exit(1)


def launch(count):
    global LOG, SOCK
    LOG = Path(f"/tmp/opencode/aeroshell-react-{os.getpid()}-{count}.log")
    SOCK = f"wayland-aero-react-{os.getpid()}-{count}"
    LOG.unlink(missing_ok=True)
    for stale in (INPUT, IPC):
        stale.unlink(missing_ok=True)
    env = os.environ.copy()
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
        env.pop(key, None)
    env.update(
        AERO_TEST_INPUT_SOCK=str(INPUT),
        AERO_WINDOW_IPC_SOCK=str(IPC),
        RUST_LOG="info",
    )
    log_handle = LOG.open("w")
    proc = subprocess.Popen(
        [
            str(COMP), "--backend", "winit", "--socket", SOCK,
            "--size", "900x600", "--command", " & ".join(["foot"] * count),
        ],
        env=env, stdout=log_handle, stderr=subprocess.STDOUT,
    )
    shell_env = dict(env)
    shell_env.pop("WAYLAND_DISPLAY", None)
    shell_env.update(
        WAYLAND_DISPLAY=SOCK,
        GDK_BACKEND="wayland",
        GTK4_LAYER_SHELL_LIB=os.environ.get("GTK4_LAYER_SHELL_LIB", "libgtk4-layer-shell.so.0"),
        RUST_LOG="info",
    )
    # Bus privado: outra instância (ex.: tty3 do humano) pode segurar o nome
    # D-Bus dev.aeroshell.shell e fazer este processo sair 0 sem ativar.
    shell = subprocess.Popen(
        ["dbus-run-session", "--", str(ROOT / "target/debug/aeroshell-shell")],
        env=shell_env, stdout=log_handle, stderr=subprocess.STDOUT,
    )
    proc._log_handle = log_handle
    return proc, shell


def stop(procs):
    for proc in procs:
        if proc.poll() is None:
            proc.terminate()
    for proc in procs:
        try:
            proc.wait(timeout=4)
        except subprocess.TimeoutExpired:
            proc.kill()
        handle = getattr(proc, "_log_handle", None)
        if handle is not None:
            handle.close()


def wait_for(predicate, seconds=20):
    deadline = time.time() + seconds
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(0.2)
    return False


def panel_windows():
    """Lista de `(id, título)` do ÚLTIMO Snapshot PUSH entregue ao painel.

    O bridge loga `BRIDGE aeroEvent event={"Snapshot":{...}}` (F2). Antes da F2
    vinha de `painel React command="list" lista=...`; mantemos o fallback.
    """
    events = re.findall(r'BRIDGE aeroEvent event=(\{.*\})$', clean(), re.MULTILINE)
    for raw in reversed(events):
        try:
            parsed = json.loads(raw)
        except json.JSONDecodeError:
            continue
        snapshot = parsed.get("Snapshot", {}).get("snapshot")
        if snapshot is not None:
            return [(w["id"], w["title"]) for w in snapshot.get("windows", [])]
    entries = re.findall(r'painel React command="list" lista=(.+)$', clean(), re.MULTILINE)
    if not entries:
        return []
    shown = []
    for token in entries[-1].split("|"):
        m = re.fullmatch(r'\s*(\d+)\s+"(.*)"\s*', token)
        if m:
            shown.append((int(m.group(1)), m.group(2)))
    return shown


def render_order():
    lines = re.findall(r'ordem de render \(frente->tras\) order=(.+)$', clean(), re.MULTILINE)
    return lines[-1] if lines else None


def interactive_rects():
    """Formato debug Rust do input_region.rs: `[(x, y, w, h), ...]`."""
    matches = re.findall(r'rects\s*=\s*\[([^\]]*)\]', clean())
    if not matches:
        return []
    return [
        dict(zip(("x", "y", "w", "h"), map(int, parts)))
        for parts in re.findall(r'\((\d+),\s*(\d+),\s*(\d+),\s*(\d+)\)', matches[-1])
    ]


def geometry():
    found = re.findall(
        r'geometry\s*=\s*Some\(Rectangle<[^>]*>\s*\{\s*x:\s*(-?\d+),\s*y:\s*(-?\d+)',
        clean(),
    )
    return tuple(map(int, found[-1])) if found else (0, 0)


def click(x, y):
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        sock.connect(str(INPUT))
        for line in (f"motion {x} {y}", "button press", "button release"):
            sock.sendall((line + "\n").encode())
            time.sleep(0.35)
    finally:
        sock.close()


def scenario(count):
    print(f"===== painel React com {count} foot =====")
    procs = launch(count)
    try:
        if not wait_for(lambda: len(panel_windows()) == count):
            fail(f"painel não listou {count} janelas; viu {panel_windows()}")
        shown = panel_windows()[-count:]
        ids = [i for i, _ in shown]
        if len(set(ids)) != count:
            fail(f"ids repetidos na lista: {shown}")
        print("painel:", shown)

        if not wait_for(lambda: bool(interactive_rects()), seconds=10):
            fail("painel não publicou retângulos interativos")
        rects = interactive_rects()
        if len(rects) < 2:
            fail(f"painel publicou só {len(rects)} retângulo(s), esperava >=2")
        gx, gy = geometry()
        before = render_order()
        if before is None:
            fail("sem ordem de render no log do compositor")

        # A ordem frente->tras varia entre execuções. Escolher o botão cujo
        # cliente está ATRÁS (último da ordem) garante que a elevação é visível.
        # Mapeamento observado: id N -> wl_surface@3[N+1] (o shell consome o 1º).
        surfaces = [int(n) for n in re.findall(r'wl_surface@3\[(\d+)\]', before)]
        if len(surfaces) != count:
            fail(f"ordem de render inesperada: {before}")
        # Sem o frame shell privilegeado os ids começam em 0; com ele, em 1.
        back_id = surfaces[-1] - min(surfaces)
        if not 0 <= back_id < count:
            fail(f"id de trás fora da lista: {back_id} (ordem={before})")
        r = rects[back_id]
        cx = gx + int(r["x"]) + int(r["w"]) // 2
        cy = gy + int(r["y"]) + int(r["h"]) // 2
        print(f"geometry={gx},{gy} rects={rects} clique botão id={back_id} em {cx},{cy}")

        event_count = lambda: len(re.findall(r'BRIDGE aeroEvent', clean()))
        events_after = event_count()
        click(cx, cy)
        if not wait_for(lambda: render_order() != before, seconds=8):
            fail(f"clique não mudou a ordem de render (antes={before})")
        after = render_order()
        print("ordem antes:", before)
        print("ordem depois:", after)

        front = after.split()[0] if after else ""
        # O índice do surface é o do render order (pode ser id ou id+1 conforme
        # exista ou não o frame shell privilegeado); usar o valor observado.
        if f"[{surfaces[-1]}]" not in front:
            fail(f"clique elegeu {front!r}, esperava o surface [{surfaces[-1]}] (id {back_id})")

        # REGRESSÃO DO TRAVAMENTO: após o `focus`, o bridge precisa continuar
        # entregando eventos. Se o main loop congelar, o stream para.
        if not wait_for(lambda: event_count() > events_after, seconds=6):
            fail("bridge parou de entregar eventos após o clique (botões mortos)")
        print("bridge continuou entregando eventos após o clique")
    finally:
        stop(procs)


if not BUNDLE.exists():
    fail(f"bundle ausente: {BUNDLE} (rode bun build antes deste teste)")
if not COMP.exists():
    fail(f"binário ausente: {COMP}")

scenario(2)
scenario(3)
print("OK: painel React listou 2 e 3 janelas; clique na 1ª linha elevou só ela")
