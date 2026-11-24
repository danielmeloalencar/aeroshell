#!/usr/bin/env python3
"""F2-shell-real: SPIKE DE DIAGNÓSTICO — não corrige nada.

Usa o `frame_pixel_probe.rs` existente (bench). Assim que o harness vê
`FRAMEWEB: frame_restored` (DOM volta a flex/block), cria o arquivo gatilho; o
probe lê UM pixel do framebuffer já mapeado no próximo frame e loga `SPIKE:`.
Junto com o `FRAME_BUFFER_COMMIT` que o compositor já emite, isso responde:
o frame commitou buffer 1292x836 ou 0x0, e o pixel da borda tem cor ou é
transparente (0x00000000)?

Saída: `SPIKE_DADO:` com o log cru. Nenhuma correção de paint é aplicada.
"""
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
RUNTIME_DIR = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-paint-spike-"))


def wait_for(predicate, timeout=15):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(0.03)
    return False


def stop(proc):
    if proc is None or proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        proc.wait(timeout=8)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            pass


def write_rules(path, policy):
    path.write_text(
        'import type { DecorationPolicyConfig } from "'
        + str(ROOT / "packages/sdk/src/generated/DecorationPolicyConfig")
        + '";\n'
        'const config: DecorationPolicyConfig = {\n'
        '  default: "Server",\n'
        '  rules: [\n'
        '    { app_id: null, class: "gedit", policy: "Client" },\n'
        '    { app_id: "foot", class: null, policy: "' + policy + '" },\n'
        '  ],\n};\nexport default config;\n'
    )


def main():
    config = OUT / "config"
    (config / "aeroshell").mkdir(parents=True, exist_ok=True)
    rules = config / "aeroshell" / "rules.ts"
    write_rules(rules, "Server")
    sock = f"wayland-aero-spike-{os.getpid()}"
    ipc = OUT / "ipc.sock"
    comp_log = OUT / "comp.log"
    pixels = OUT / "pixels"
    pixels.mkdir()
    trigger = OUT / "trigger"
    trigger_output = OUT / "trigger-output"
    env = {**os.environ, "RUST_LOG": "warn", "AERO_WINDOW_IPC_SOCK": str(ipc),
           "AERO_PIXEL_DUMP_DIR": str(pixels),
           "AERO_PIXEL_SPIKE_TRIGGER": str(trigger),
           "AERO_PIXEL_SPIKE_OUTPUT_TRIGGER": str(trigger_output),
           "AERO_PIXEL_SPIKE_X": "0", "AERO_PIXEL_SPIKE_Y": "0",
           "XDG_CONFIG_HOME": str(config), "HOME": str(OUT)}
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD", "AERO_DECORATION_RULES"):
        env.pop(key, None)

    comp = runtime = shell = None
    feet = []
    try:
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "winit",
             "--socket", sock, "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, env=env, stdout=comp_log.open("wb"),
            stderr=subprocess.STDOUT, start_new_session=True,
        )
        if not wait_for(lambda: ipc.exists() and (RUNTIME_DIR / sock).exists(), 15):
            print("FALHA: compositor não subiu", file=sys.stderr)
            return 1
        runtime = subprocess.Popen(
            ["bun", str(ROOT / "packages/runtime/src/policy.ts")], cwd=ROOT,
            env={**env, "AERO_WINDOW_IPC_SOCK": str(ipc)},
            stdout=(OUT / "runtime.log").open("wb"), stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        wait_for(lambda: "POLICY_TS READY" in (OUT / "runtime.log").read_text(errors="ignore"), 15)
        shell = subprocess.Popen(
            ["dbus-run-session", "--", str(ROOT / "target/debug/aeroshell-shell")], cwd=ROOT,
            env={**env, "WAYLAND_DISPLAY": sock, "GDK_BACKEND": "wayland",
                 "GTK4_LAYER_SHELL_LIB": os.environ.get("GTK4_LAYER_SHELL_LIB", "libgtk4-layer-shell.so.0")},
            stdout=(OUT / "shell.log").open("wb"), stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        time.sleep(3)
        for _ in range(2):
            feet.append(subprocess.Popen(
                ["foot"], cwd=ROOT, env={**env, "WAYLAND_DISPLAY": sock},
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True,
            ))
            time.sleep(1)
        time.sleep(3)

        def ipc_list():
            try:
                with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
                    s.settimeout(2)
                    s.connect(str(ipc))
                    s.sendall(b"list\n")
                    return s.recv(8192).decode(errors="ignore")
            except OSError:
                return ""

        m = re.search(r"W 0 (-?\d+) (-?\d+) (\d+) (\d+)", ipc_list())
        if not m:
            print("FALHA: geometria do foot indisponível", file=sys.stderr)
            return 1
        x, y, w, h = (int(v) for v in m.groups())
        # Borda esquerda do foot (decorada no estado Server), amostrada pelo probe.
        px, py = max(x - 3, 0), y + 40
        print(f"geometria foot: ({x},{y},{w},{h}) -> pixel da borda ({px},{py})")

        write_rules(rules, "None")
        if not wait_for(lambda: "FRAMEWEB: csd_applied" in comp_log.read_text(errors="ignore"), 12):
            print("FALHA: transição para None não aconteceu", file=sys.stderr)
            return 1
        write_rules(rules, "Server")
        if not wait_for(
            lambda: "FRAMEWEB: frame_restored" in comp_log.read_text(errors="ignore"), 12
        ):
            print("FALHA: frame_restored não aconteceu", file=sys.stderr)
            return 1
        # Gatilho imediato, no mesmo instante em que o DOM volta.
        trigger.write_text(f"{px} {py}\n")
        trigger_output.write_text(f"{px} {py}\n")
        if not wait_for(lambda: "SPIKE:" in comp_log.read_text(errors="ignore"), 8):
            print("FALHA: probe não gerou SPIKE", file=sys.stderr)
            return 1
        time.sleep(0.3)
        lines = comp_log.read_text(errors="ignore").splitlines()
        spike = [line for line in lines if "SPIKE" in line]
        commits = [line for line in lines if "FRAME_BUFFER_COMMIT" in line][-6:]
        restored = [line for line in lines if "FRAMEWEB: frame_restored" in line][-2:]
        print("=== SPIKE_DADO (cru) ===")
        for line in spike + restored + commits:
            print(line[:240])
        buffer_ok = any("SPIKE_BUFFER: buffer=Some" in line for line in spike)
        print("VEREDITO_SPIKE: " + ("OK" if buffer_ok else "FALHA"))
        print(f"artefatos: {OUT}")
        print("SPIKE: diagnóstico concluído; nenhuma correção de paint aplicada neste harness.")
        return 0 if buffer_ok else 1
    finally:
        for proc in [*feet, runtime, shell]:
            stop(proc)
        stop(comp)


if __name__ == "__main__":
    sys.exit(main())
