#!/usr/bin/env python3
"""F2-hotreload com gedit CSD real: a heurística de geometria NÃO pode vencer
a decisão explícita da política (juiz visual, 2026-11-24).

Baseline: `csd <id>` continua 1 após a política virar Server (por
`from_geometry`), o shell não restaura a barra → VEREDITO: FALHA.

Pre-requisito: cargo build -p aeroshell-compositor --features bench
               cargo build --workspace --examples
Uso: python3 tests/integration/f2_hotreload_gedit_csd.py
"""
import os
import pathlib
import re
import signal
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNTIME = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-hotreload-csd-"))


def wait_for(predicate, timeout=15):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


def ipc(ipc_path, command):
    try:
        out = subprocess.run(
            ["socat", "-", f"UNIX-CONNECT:{ipc_path}"],
            input=f"{command}\n", text=True, capture_output=True, timeout=3,
        )
        return out.stdout
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return ""


def stop(proc):
    if proc is None or proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait(timeout=5)


def write_rules(path, policy):
    path.write_text(
        'import type { DecorationPolicyConfig } from "'
        + str(ROOT / "packages/sdk/src/generated/DecorationPolicyConfig")
        + '";\n'
        "const config: DecorationPolicyConfig = {\n"
        '  default: "Server",\n'
        f'  rules: [{{ app_id: null, class: "gedit", policy: "{policy}" }}],\n'
        "};\nexport default config;\n"
    )


def main():
    sock = f"wayland-aero-f2-hotreload-csd-{os.getpid()}"
    ipc_path = str(OUT / "ipc.sock")
    rules = OUT / "rules.ts"
    comp_log = OUT / "comp.log"
    runtime_log = OUT / "runtime.log"
    write_rules(rules, "Client")

    env = {**os.environ, "RUST_LOG": "debug", "AERO_WINDOW_IPC_SOCK": ipc_path}
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "LD_PRELOAD"):
        env.pop(key, None)

    comp = runtime = client = None
    try:
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
             "--socket", sock, "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, stdout=comp_log.open("wb"), stderr=subprocess.STDOUT, env=env,
            start_new_session=True,
        )
        wait_for(lambda: (RUNTIME / sock).exists(), 15)
        wait_for(lambda: pathlib.Path(ipc_path).exists(), 10)

        runtime = subprocess.Popen(
            ["bun", str(ROOT / "packages/runtime/src/policy.ts")],
            cwd=ROOT, stdout=runtime_log.open("wb"), stderr=subprocess.STDOUT,
            env={**os.environ, "AERO_WINDOW_IPC_SOCK": ipc_path,
                 "AERO_DECORATION_RULES": str(rules)},
            start_new_session=True,
        )
        wait_for(lambda: "POLICY_TS READY" in runtime_log.read_text(errors="replace"))

        # D-Bus próprio: senão o gedit delega a abertura da janela à instância
        # do host (ou à do juiz visual) e nada mapeia neste compositor.
        client = subprocess.Popen(
            ["dbus-run-session", "--", "gedit"], cwd=ROOT,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            env={**os.environ, "WAYLAND_DISPLAY": sock}, start_new_session=True,
        )
        wait_for(lambda: "W 0" in ipc(ipc_path, "list"), 20)

        def clog():
            return comp_log.read_text(errors="replace")

        def frame_event(action):
            text = clog()
            if action == "frame_restored":
                # `eprintln!` concorrente (FSCALE_FRAME) pode intercalar a linha:
                # exigir window_id=0 na MESMA linha daria falso negativo.
                return (
                    "FRAMEWEB: frame_restored" in text
                    and bool(re.search(r"FRAMEWEB: csd_restored[^\n]*window_id=0", text))
                )
            return bool(re.search(rf"FRAMEWEB: {action}[^\n]*window_id=0", text))

        initial = wait_for(lambda: frame_event("csd_applied"))
        csd_initial = ipc(ipc_path, "csd 0").strip()

        write_rules(rules, "Server")
        server_ok = wait_for(
            lambda: "policy=Server" in runtime_log.read_text(errors="replace")
            and ipc(ipc_path, "csd 0").strip() == "CSD 0 0"
        )
        restored = wait_for(lambda: frame_event("frame_restored"))

        # O hide NÃO pode depender da janela de 5s do csd_probe_until do startup:
        # no juiz a janela está aberta há minutos. Deixar o probe expirar antes
        # de voltar a Client para o teste medir o caminho real do hot-reload.
        applied_before = len(re.findall(r"FRAMEWEB: csd_applied", clog()))
        time.sleep(6)
        write_rules(rules, "Client")
        client_ok = wait_for(
            lambda: "policy=Client" in runtime_log.read_text(errors="replace")
            and ipc(ipc_path, "csd 0").strip() == "CSD 0 1"
            and len(re.findall(r"FRAMEWEB: csd_applied", clog())) > applied_before
        )

        print("=== runtime (cru) ===")
        print(runtime_log.read_text(errors="replace"), end="")
        print("=== comp CSD/frame (cru) ===")
        for line in clog().splitlines():
            if "CSD" in line or "FRAMEWEB: csd_" in line or "FRAMEWEB: frame_restored" in line:
                print(line)

        checks = {
            "gedit CSD escondeu barra": initial and csd_initial == "CSD 0 1",
            "decisão Server vence heurística de geometria": server_ok,
            "shell restaurou a barra no DOM": restored,
            "voltar a Client esconde de novo": client_ok,
            "runtime vivo": runtime.poll() is None,
            "zero erros de protocolo": "ProtocolError" not in clog(),
        }
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} csd/{label}")
        print("VEREDITO: " + ("OK" if all(checks.values()) else "FALHA"))
        return 0 if all(checks.values()) else 1
    finally:
        stop(client)
        stop(runtime)
        stop(comp)


if __name__ == "__main__":
    sys.exit(main())
