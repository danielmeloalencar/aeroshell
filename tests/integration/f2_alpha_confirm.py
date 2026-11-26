#!/usr/bin/env python3
"""F2-alpha-confirm: região restringe; alfa decide dentro dela.

Pré-requisitos (bench POR ÚLTIMO):
  cargo build --workspace --examples
  cargo build -p aeroshell-shell
  cargo build -p aeroshell-compositor --features bench

Uso: python3 tests/integration/f2_alpha_confirm.py
A baseline anterior ao hit-test por alfa deve FALHAR nos pontos transparentes.
"""
import json
import math
import os
import pathlib
import re
import signal
import socket
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
TARGET = os.environ.get("AERO_TARGET", "debug")
BIN = ROOT / "target" / TARGET
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-alpha-", dir="/tmp/opencode"))
IPC = OUT / "ipc.sock"
INPUT = OUT / "input.sock"
LOG = OUT / "compositor.log"
HOST_LOG = OUT / "host.log"
RUNTIME = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
SOCKET = f"wayland-aero-f2-alpha-{os.getpid()}"
OUTPUT = (640, 480)
ANSI = re.compile(r"\x1b\[[0-9;]*m")
ALPHA_HTML = """<!doctype html><html><head><style>
html,body{width:100%;height:100%;margin:0;background:transparent;overflow:hidden}
.borda{position:absolute;inset:0;border:60px solid #00d000;box-sizing:border-box;pointer-events:none}
</style></head><body><div class=borda></div></body></html>"""


def clean(path=LOG):
    return ANSI.sub("", path.read_text(errors="replace")) if path.exists() else ""


def wait_for(predicate, timeout=20):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if predicate():
                return True
        except (OSError, ValueError, KeyError, IndexError, json.JSONDecodeError):
            pass
        time.sleep(0.05)
    return False


def command(line, dot=False):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.settimeout(6)
        sock.connect(str(IPC))
        sock.sendall((line + "\n").encode())
        lines = []
        for raw in sock.makefile("r", encoding="utf-8"):
            raw = raw.rstrip("\n")
            lines.append(raw)
            if (dot and raw == ".") or (not dot and raw.startswith(("OK", "ERR"))):
                break
        return lines


def windows():
    result = {}
    for line in command("list", dot=True):
        match = re.match(
            r"W (\d+) (-?\d+) (-?\d+) (\d+) (\d+) (\"(?:[^\"\\]|\\.)*\") .*mode=(\w+)", line
        )
        if match:
            result[int(match[1])] = {
                "x": int(match[2]), "y": int(match[3]),
                "width": int(match[4]), "height": int(match[5]), "mode": match[7],
            }
    return result


def stop(proc):
    if proc is None or proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        proc.wait(timeout=6)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait(timeout=5)


def surface_spec(kind, region):
    return {
        "id": "alpha", "kind": kind, "output": 0,
        "anchors": {"left": True, "right": True, "top": True, "bottom": True},
        "margins": [0, 0, 0, 0], "exclusive_zone": 0,
        "layer": {"desktop": "background", "panel": "top", "overlay": "overlay"}[kind],
        "keyboard_mode": "none" if kind == "desktop" else "ondemand",
        "input_region": region, "html": ALPHA_HTML,
    }


def surface_command(action, kind="overlay", region=None):
    if region is None:
        region = [{"x": 0, "y": 0, "width": OUTPUT[0], "height": OUTPUT[1]}]
    spec = surface_spec(kind, region)
    return command(f"surface-{action} " + json.dumps(spec, separators=(",", ":")))


def percentile(values, fraction):
    if not values:
        return None
    values = sorted(values)
    return values[max(0, math.ceil(len(values) * fraction) - 1)]


def main():
    comp = host = foot2 = None
    checks = {}
    input_sock = None
    try:
        env = {
            **os.environ, "RUST_LOG": "info", "AERO_WINDOW_IPC_SOCK": str(IPC),
            "AERO_TEST_INPUT_SOCK": str(INPUT), "AERO_SURFACE_HOST": "1",
        }
        for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD", "AERO_DEV_ORIGIN"):
            env.pop(key, None)
        comp = subprocess.Popen(
            [str(BIN / "aeroshell-compositor"), "--backend", "headless", "--socket", SOCKET,
             "--size", "640x480", "--command", "foot -w 300x220",
             "--shell", str(BIN / "examples/alpha_frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, env=env, stdout=LOG.open("wb"), stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        checks["compositor/IPC prontos"] = wait_for(
            lambda: (RUNTIME / SOCKET).exists() and IPC.exists() and INPUT.exists(), 15
        )
        if not checks["compositor/IPC prontos"]:
            raise RuntimeError("compositor não ficou pronto")
        host = subprocess.Popen(
            ["dbus-run-session", "--", str(BIN / "aeroshell-shell")], cwd=ROOT,
            env={**env, "WAYLAND_DISPLAY": SOCKET, "GDK_BACKEND": "wayland",
                 "GTK4_LAYER_SHELL_LIB": "libgtk4-layer-shell.so.0"},
            stdout=HOST_LOG.open("wb"), stderr=subprocess.STDOUT, start_new_session=True,
        )
        checks["host e primeiro frame prontos"] = wait_for(
            lambda: "SURFACE_HOST pronto" in clean(HOST_LOG)
            and 0 in windows() and "FRAMEWEB: title_applied window_id=0" in clean(), 45
        )
        if not checks["host e primeiro frame prontos"]:
            raise RuntimeError("host/frame não ficaram prontos")
        input_sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        input_sock.connect(str(INPUT))

        def send(line):
            input_sock.sendall((line + "\n").encode())

        def probe(x, y):
            mark = len(clean())
            send(f"motion {x} {y}")
            found = []

            def collect():
                found.clear()
                for line in clean()[mark:].splitlines():
                    if "TESTE ponteiro motion" in line and "under=" in line:
                        found.append(line)
                return bool(found)

            if not wait_for(collect, 5):
                return None
            line = found[-1]
            if "under=None" in line:
                return {"surface": None, "origin": None, "raw": line}
            match = re.search(
                r"under=Some\(\(ObjectId\(wl_surface@\d+\[\d+\], (\d+)\), "
                r"Point<[^>]+> \{ x: (-?[\d.]+), y: (-?[\d.]+) \}\)", line
            )
            if not match:
                return {"surface": "malformed", "origin": None, "raw": line}
            return {
                "surface": match.group(1),
                "origin": (float(match.group(2)), float(match.group(3))), "raw": line,
            }

        def click(x, y):
            hit = probe(x, y)
            send("button press")
            time.sleep(0.12)
            send("button release")
            time.sleep(0.12)
            return hit

        checks["primeiro cliente com tamanho determinístico"] = command("resize 0 300 220") == ["OK"] and wait_for(
            lambda: windows().get(0, {}).get("width", 0) <= 300
            and windows().get(0, {}).get("height", 0) <= 220, 15
        )
        first = windows()[0]
        first_client_hit = probe(first["x"] + first["width"] // 2, first["y"] + first["height"] // 2)
        checks["cliente inicial identificado"] = bool(
            first_client_hit and first_client_hit["surface"] not in (None, "malformed")
        )
        first_client = first_client_hit["surface"]
        print(f"=== cliente inicial: {first_client} ===")

        opaque = (30, 30)
        transparent = (320, 240)
        over_client = (320, 240)
        for kind in ("desktop", "panel", "overlay"):
            action = "create" if kind == "desktop" else "update"
            created_before = clean(HOST_LOG).count("SURFACE_HOST criado id=alpha")
            checks[f"{kind}: {action} responde OK"] = surface_command(action, kind) == ["OK"]
            checks[f"{kind}: WebView pronta"] = wait_for(
                lambda: clean(HOST_LOG).count("SURFACE_HOST criado id=alpha") >= created_before + (action == "create"), 15
            )
            time.sleep(1.2)
            hit_opaque = click(*opaque)
            hit_transparent = click(*transparent)
            hit_client = click(*over_client)
            print(f"=== {kind}: opaco={hit_opaque} transparente={hit_transparent} cliente={hit_client} ===")
            checks[f"{kind}: opaco captura"] = bool(
                hit_opaque and hit_opaque["surface"] not in (None, "malformed", first_client)
            )
            checks[f"{kind}: transparente dentro da região repassa"] = bool(
                hit_transparent and hit_transparent["surface"] == first_client
            )
            checks[f"{kind}: transparente chega ao cliente"] = bool(
                hit_client and hit_client["surface"] == first_client
            )
            time.sleep(0.2)

        # Overlay fullscreen + ordem região→alfa: fora da região não pode ler pixel.
        checks["overlay fullscreen criado"] = surface_command("update", "overlay") == ["OK"]
        time.sleep(1.2)
        restricted = [{"x": 0, "y": 0, "width": OUTPUT[0], "height": 60}]
        checks["região restrita aplicada"] = surface_command("update", "overlay", restricted) == ["OK"]
        time.sleep(0.5)
        mark = len(clean())
        outside = click(*over_client)
        outside_log = clean()[mark:]
        checks["fora da região repassa sem readback"] = bool(
            outside and outside["surface"] == first_client and "ALPHA_READBACK" not in outside_log
        )
        # O host retém surfaces destruídas (`retired`); região vazia isola o cenário
        # sem transformar lifecycle fora do cartão em pré-requisito da correção de alfa.
        checks["overlay não intercepta os frames"] = surface_command("update", "overlay", []) == ["OK"]
        time.sleep(0.5)

        # Dois clientes reais: o segundo fica sob a decoração transparente do primeiro.
        foot2 = subprocess.Popen(
            ["foot", "-w", "300x220"], cwd=ROOT, env={**env, "WAYLAND_DISPLAY": SOCKET},
            stdout=LOG.open("ab"), stderr=subprocess.STDOUT, start_new_session=True,
        )
        checks["segundo cliente/frame prontos"] = wait_for(
            lambda: 1 in windows() and "FRAMEWEB: title_applied window_id=1" in clean(), 45
        )
        checks["segundo cliente com tamanho determinístico"] = command("resize 1 300 220") == ["OK"] and wait_for(
            lambda: windows().get(1, {}).get("width", 0) <= 300
            and windows().get(1, {}).get("height", 0) <= 220, 15
        )
        checks["segundo cliente posicionado"] = command("move 1 60 30") == ["OK"]
        time.sleep(0.8)
        second_hit = probe(250, 40)
        second_client = second_hit["surface"] if second_hit else None
        command("focus 0")
        time.sleep(0.5)
        first = windows()[0]
        origin = (first["x"] - 6, first["y"] - 30)
        target = (first["width"] + 12, first["height"] + 36)
        frame_opaque = click(origin[0] + 16, origin[1] + 15)
        frame_transparent = click(250, 60)
        frame_surface = frame_opaque["surface"] if frame_opaque else None
        checks["frame: opaco captura"] = frame_surface not in (None, "malformed", first_client)
        checks["frame: transparente repassa"] = bool(
            frame_transparent and frame_transparent["surface"] not in (frame_surface, "malformed")
        )
        print(f"=== frame: surface={frame_surface} cliente2={second_client} transparente={frame_transparent} ===")
        command("focus 0")
        time.sleep(0.4)

        canvas_matches = re.findall(r"FRAME_CONFIGURE[^\n]*window_id=0[^\n]*canvas=(\d+)x(\d+)", clean())
        canvas = tuple(map(int, canvas_matches[-1])) if canvas_matches else (OUTPUT[0] + 12, OUTPUT[1] + 36)
        right = (origin[0] + target[0] - 3, origin[1] + 100)
        right_transparent = (origin[0] + target[0] - 50, origin[1] + 15)
        bottom = (origin[0] + 16, origin[1] + target[1] - 3)
        bottom_transparent = (origin[0] + 150, origin[1] + target[1] - 3)
        hit_right = click(*right)
        hit_right_transparent = click(*right_transparent)
        hit_bottom = click(*bottom)
        hit_bottom_transparent = click(*bottom_transparent)
        expected_right_origin = (origin[0] - (canvas[0] - target[0]), float(origin[1]))
        expected_bottom_origin = (float(origin[0]), origin[1] - (canvas[1] - target[1]))
        print(f"=== caps: origin={origin} target={target} direito={hit_right}/{hit_right_transparent} inferior={hit_bottom}/{hit_bottom_transparent} ===")
        checks["9-slice: cap direito opaco e origem canvas"] = bool(
            hit_right and hit_right["surface"] == frame_surface and hit_right["origin"] == expected_right_origin
        )
        checks["9-slice: cap direito transparente repassa"] = bool(
            hit_right_transparent and hit_right_transparent["surface"] != frame_surface
        )
        checks["9-slice: cap inferior opaco e origem canvas"] = bool(
            hit_bottom and hit_bottom["surface"] == frame_surface and hit_bottom["origin"] == expected_bottom_origin
        )
        checks["9-slice: cap inferior transparente repassa"] = bool(
            hit_bottom_transparent and hit_bottom_transparent["surface"] != frame_surface
        )

        checks["fullscreen xdg ativado"] = command("fullscreen 0") == ["OK"] and wait_for(
            lambda: windows().get(0, {}).get("mode") == "fullscreen"
        )
        fullscreen_hit = click(250, 60)
        checks["fullscreen xdg: frame não captura"] = bool(
            fullscreen_hit and fullscreen_hit["surface"] == first_client
        )

        # Cache: commit novo + motion/press no mesmo pixel exigem exatamente um readback.
        updated = surface_spec("overlay", [{"x": 0, "y": 0, "width": OUTPUT[0], "height": OUTPUT[1]}])
        updated["html"] = ALPHA_HTML.replace("#00d000", "#00d001")
        raw_update = command("surface-update " + json.dumps(updated, separators=(",", ":")))
        checks["overlay para cache/S3 criado"] = raw_update == ["OK"]
        time.sleep(1.2)
        mark = len(clean())
        click(*opaque)
        cache_log = clean()[mark:]
        cache_reads = len(re.findall(r"ALPHA_READBACK", cache_log))
        checks["cache motion→press: um readback"] = cache_reads == 1
        checks["todo readback é 1×1; zero full-frame"] = cache_reads > 0 and all(
            "size=1x1" in line and "full_frame=0" in line
            for line in cache_log.splitlines() if "ALPHA_READBACK" in line
        )

        # S3: 2000 motions em ~2 s, alternando opaco/transparente para medir o pior caminho.
        mark = len(clean())
        count = 2000
        started = time.monotonic()
        next_at = started
        for index in range(count):
            point = opaque if index % 2 == 0 else over_client
            send(f"motion {point[0]} {point[1]}")
            next_at += 0.001
            delay = next_at - time.monotonic()
            if delay > 0:
                time.sleep(delay)
        elapsed = time.monotonic() - started
        wait_for(lambda: len(re.findall(r"HIT_TEST duration_ns=", clean()[mark:])) >= count, 20)
        s3_log = clean()[mark:]
        durations = [int(value) for value in re.findall(r"HIT_TEST duration_ns=(\d+)", s3_log)][:count]
        p99_ns = percentile(durations, 0.99)
        reads = [line for line in s3_log.splitlines() if "ALPHA_READBACK" in line]
        rate = len(durations) / elapsed if elapsed else 0
        print(f"=== S3: motions={len(durations)} taxa={rate:.1f}/s p99={p99_ns}ns readbacks={len(reads)} ===")
        checks["S3: ~1000 motions/s processados"] = len(durations) == count and 850 <= rate <= 1150
        checks["S3: p99 por motion <1 ms"] = p99_ns is not None and p99_ns < 1_000_000
        checks["S3: zero readback full-frame"] = bool(reads) and all(
            "size=1x1" in line and "full_frame=0" in line for line in reads
        )
        checks["zero erros de protocolo"] = "ProtocolError" not in clean()
    except (OSError, RuntimeError, IndexError, AttributeError, json.JSONDecodeError, subprocess.TimeoutExpired) as err:
        checks["execução sem exceção"] = False
        print(f"EXCEÇÃO: {err}")
    finally:
        print(f"LOGS: {OUT}")
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {label}")
        verdict = "OK" if checks and all(checks.values()) else "FALHA"
        print("VEREDITO: " + verdict)
        if input_sock is not None:
            input_sock.close()
        for proc in (foot2, host, comp):
            stop(proc)
    return 0 if verdict == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
