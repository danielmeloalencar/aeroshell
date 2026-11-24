#!/usr/bin/env python3
"""F2-decoration-policy-hotreload: teste-first do reload sem restart.

Baseline (sem watcher/reload): editar o .ts não gera `POLICY_TS RELOAD` nem
nova decisão → VEREDITO: FALHA.

Pre-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build --workspace --examples
  bun run generate:types

Uso: python3 tests/integration/f2_hotreload_decoration_policy.py
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
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-hotreload-"))
APP_ID = "aero-hotreload"


def wait_for(predicate, timeout=12):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


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
        f'  rules: [{{ app_id: "{APP_ID}", class: null, policy: "{policy}" }}],\n'
        "};\nexport default config;\n"
    )


def main():
    sock = f"wayland-aero-f2-hotreload-{os.getpid()}"
    ipc = str(OUT / "ipc.sock")
    rules = OUT / "rules.ts"
    comp_log = OUT / "comp.log"
    runtime_log = OUT / "runtime.log"
    write_rules(rules, "Server")

    env = {**os.environ, "RUST_LOG": "debug", "AERO_WINDOW_IPC_SOCK": ipc}
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET"):
        env.pop(key, None)

    comp = runtime = client = None
    try:
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
             "--socket", sock,
             "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, stdout=comp_log.open("wb"), stderr=subprocess.STDOUT, env=env,
        )
        wait_for(lambda: (RUNTIME / sock).exists(), 15)
        wait_for(lambda: pathlib.Path(ipc).exists(), 10)

        run_env = {
            **os.environ,
            "AERO_WINDOW_IPC_SOCK": ipc,
            "AERO_DECORATION_RULES": str(rules),
        }
        runtime = subprocess.Popen(
            ["bun", str(ROOT / "packages/runtime/src/policy.ts")],
            cwd=ROOT, stdout=runtime_log.open("wb"), stderr=subprocess.STDOUT,
            env=run_env, start_new_session=True,
        )
        wait_for(lambda: "POLICY_TS READY" in runtime_log.read_text(errors="replace"))

        client_err = OUT / "client.err"
        client = subprocess.Popen(
            [str(ROOT / "target/debug/examples/policy_client"), APP_ID],
            cwd=ROOT, stdout=subprocess.DEVNULL, stderr=client_err.open("wb"),
            env={**os.environ, "WAYLAND_DISPLAY": sock, "AERO_POLICY_CLIENT_SECS": "3600"},
            start_new_session=True,
        )
        ready = wait_for(
            lambda: "POLICY_CLIENT READY" in client_err.read_text(errors="replace")
        )
        comp_pid, runtime_pid = comp.pid, runtime.pid

        def rlog():
            return runtime_log.read_text(errors="replace")

        def clog():
            return comp_log.read_text(errors="replace")

        def decisions():
            return re.findall(
                r"POLICY_TS DECISION id=(\d+) app_id=(\S+) policy=(\w+) source=(\w+)", rlog()
            )

        def applied():
            return re.findall(
                r"POLICY t=\d+ window_id=(\d+) decision=(\w+) fallback=(\w+) reason=(\w+)", clog()
            )

        def suppressed():
            return re.findall(
                r"POLICY_RENDER t=\d+ window_id=(\d+) suppressed=policy_none", clog()
            )

        initial_ok = wait_for(
            lambda: any(d[2].lower() == "server" and d[3] == "rule" for d in decisions())
            and any(a[1] == "server" and a[2] == "false" for a in applied())
        )

        # 1) Editar o .ts com o compositor e o runtime de pé.
        write_rules(rules, "None")
        reload_ok = wait_for(
            lambda: "POLICY_TS RELOAD" in rlog()
            and any(d[2].lower() == "none" for d in decisions())
            and any(a[1] == "none" and a[2] == "false" for a in applied())
            and bool(suppressed())
        )
        suppressed_before_bad = len(suppressed())

        # 2) Sintaxe inválida: última boa preservada, nenhuma decisão nova.
        decisions_before_bad = len(decisions())
        rules.write_text("export default { default: ; syntax error }")
        bad_ok = wait_for(lambda: "POLICY_TS RELOAD ERROR" in rlog())
        time.sleep(1.0)
        last_good_kept = (
            len(decisions()) == decisions_before_bad
            and applied()[-1][1] == "none"
            and len(suppressed()) == suppressed_before_bad
            and runtime.poll() is None
        )

        # 3) Corrigir: volta a Server na MESMA janela, sem suppressed novo.
        write_rules(rules, "Server")
        repaired_ok = wait_for(
            lambda: any(d[2].lower() == "server" for d in decisions()[decisions_before_bad:])
            and any(a[1] == "server" and a[2] == "false" for a in applied())
        )
        time.sleep(1.0)
        suppressed_after_repair = len(suppressed()) == suppressed_before_bad

        print("=== runtime TS (cru) ===")
        print(rlog(), end="")
        print("=== compositor POLICY (cru) ===")
        for line in clog().splitlines():
            if "POLICY" in line or "DecorationRequest" in line:
                print(line)

        checks = {
            "cliente pronto": ready,
            "decisão inicial server aplicada": initial_ok,
            "editar .ts reaplica none sem restart": reload_ok,
            "erro de sintaxe logado": bad_ok,
            "últimas boas mantidas no erro": last_good_kept,
            "correção reaplica server": repaired_ok,
            "frame voltou (sem suppressed novo)": suppressed_after_repair,
            "mesmo PID do compositor": comp.pid == comp_pid and comp.poll() is None,
            "mesmo PID do runtime": runtime.pid == runtime_pid and runtime.poll() is None,
            "nenhum timeout": "reason=timeout" not in clog(),
            "zero erros de protocolo": "ProtocolError" not in clog(),
        }
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} hotreload/{label}")
        print("VEREDITO: " + ("OK" if all(checks.values()) else "FALHA"))
        return 0 if all(checks.values()) else 1
    finally:
        stop(client)
        stop(runtime)
        stop(comp)


if __name__ == "__main__":
    sys.exit(main())
