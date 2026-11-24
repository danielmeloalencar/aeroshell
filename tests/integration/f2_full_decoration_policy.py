#!/usr/bin/env python3
"""F2-decoration-policy-ts: teste-first da política completa em TS/Bun.

Baseline (sem runtime TS): nenhum `POLICY_TS READY`/`DECISION`, nenhuma decisão
aplicada e nenhuma regra editada em TS → VEREDITO: FALHA.

Pre-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build --workspace --examples
  bun run generate:types

Uso: python3 tests/integration/f2_full_decoration_policy.py
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
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-full-policy-"))

CLIENTS = {
    "policy": ["gedit", "foot", "aero-no-deco", "aero-unknown"],
    "no-runtime": ["aero-no-deco"],
    "timeout": ["aero-no-deco"],
    "edited-rules": ["aero-ts-new"],
}


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


def run_case(name, mode):
    sock = f"wayland-aero-f2-full-{name}-{os.getpid()}"
    dir_ = OUT / name
    dir_.mkdir(parents=True, exist_ok=True)
    ipc = str(dir_ / "ipc.sock")
    comp_log = dir_ / "comp.log"
    runtime_log = dir_ / "runtime.log"
    env = {**os.environ, "RUST_LOG": "debug", "AERO_WINDOW_IPC_SOCK": ipc}
    if mode == "timeout":
        env["AERO_POLICY_TIMEOUT_MS"] = "200"
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET"):
        env.pop(key, None)

    # Regra nova escrita em TS, sem tocar no Rust: só o runtime é reiniciado.
    rules_path = None
    if mode == "edited-rules":
        rules_path = dir_ / "rules-edited.ts"
        rules_path.write_text(
            'import type { DecorationPolicyConfig } from "'
            + str(ROOT / "packages/sdk/src/generated/DecorationPolicyConfig")
            + '";\n'
            'const config: DecorationPolicyConfig = {\n'
            '  default: "Server",\n'
            '  rules: [{ app_id: "aero-ts-new", class: null, policy: "None" }],\n'
            '};\nexport default config;\n'
        )

    comp = runtime = None
    clients = []
    try:
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
             "--socket", sock,
             "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, stdout=comp_log.open("wb"), stderr=subprocess.STDOUT, env=env,
        )
        deadline = time.time() + 15
        while time.time() < deadline and not (RUNTIME / sock).exists():
            time.sleep(0.05)
        deadline = time.time() + 10
        while time.time() < deadline and not pathlib.Path(ipc).exists():
            time.sleep(0.05)
        time.sleep(0.2)

        if mode != "no-runtime":
            run_env = {**os.environ, "AERO_WINDOW_IPC_SOCK": ipc}
            if rules_path is not None:
                run_env["AERO_DECORATION_RULES"] = str(rules_path)
            if mode == "timeout":
                run_env["AERO_POLICY_NO_REPLY"] = "1"
            runtime = subprocess.Popen(
                ["bun", str(ROOT / "packages/runtime/src/policy.ts")],
                cwd=ROOT, stdout=runtime_log.open("wb"), stderr=subprocess.STDOUT,
                env=run_env, start_new_session=True,
            )
            wait_for(lambda: "POLICY_TS READY" in runtime_log.read_text(errors="replace"))

        for app_id in CLIENTS[mode]:
            err = dir_ / f"client-{app_id}.err"
            client = subprocess.Popen(
                [str(ROOT / "target/debug/examples/policy_client"), app_id],
                cwd=ROOT, stdout=subprocess.DEVNULL, stderr=err.open("wb"),
                env={**os.environ, "WAYLAND_DISPLAY": sock}, start_new_session=True,
            )
            clients.append(client)
        ready = wait_for(
            lambda: all(
                "POLICY_CLIENT READY" in (dir_ / f"client-{a}.err").read_text(errors="replace")
                for a in CLIENTS[mode]
            )
        )
        ready_at = time.time()
        fallback_ms = None
        deadline = time.time() + 2
        while time.time() < deadline and fallback_ms is None:
            if "reason=no-runtime" in comp_log.read_text(errors="replace"):
                fallback_ms = (time.time() - ready_at) * 1000
            time.sleep(0.01)
        time.sleep(2)

        log = comp_log.read_text(errors="replace")
        rlog = runtime_log.read_text(errors="replace") if runtime is not None else ""
        print(f"=== {name}: runtime TS (cru) ===")
        print(rlog, end="")
        print(f"=== {name}: compositor POLICY cru ===")
        for line in log.splitlines():
            if "POLICY" in line or "DecorationRequest" in line:
                print(line)

        policies = {}
        for window_id, text in re.findall(r"POLICY t=\d+ window_id=(\d+) ([^\n]+)", log):
            policies[window_id] = text
        suppressed = {
            window_id
            for window_id, flag in re.findall(
                r"POLICY_RENDER t=\d+ window_id=(\d+) suppressed=(\w+)", log
            )
            if flag == "policy_none"
        }
        decisions = {}
        for window_id, app_id, policy, source in re.findall(
            r"POLICY_TS DECISION id=(\d+) app_id=(\S+) policy=(\w+) source=(\w+)", rlog
        ):
            decisions[app_id] = (window_id, policy.lower(), source)

        checks = {"clientes prontos": ready}
        if mode == "policy":
            expected = {
                "gedit": ("client", "rule"),
                "foot": ("server", "rule"),
                "aero-no-deco": ("none", "rule"),
                "aero-unknown": ("server", "default"),
            }
            checks.update({
                "runtime TS anunciou policy-ready": "POLICY_TS READY" in rlog
                    and "ready_reply=OK" in rlog,
                "pedidos no stream": "POLICY_TS REQUEST" in rlog
                    and "kind=decoration_request" in log,
                "regras TS honradas": all(
                    app_id in decisions and decisions[app_id][1:] == expected[app_id]
                    for app_id in expected
                ),
                "compositor aplicou decisões explícitas": all(
                    app_id in decisions
                    and f"decision={expected[app_id][0]} fallback=false" in policies.get(decisions[app_id][0], "")
                    for app_id in expected
                ),
                "none suprimiu frame": any(
                    a == "aero-no-deco" and decisions[a][0] in suppressed for a in decisions
                ),
                "foot/gedit mantiveram frame": all(
                    a in decisions and decisions[a][0] not in suppressed
                    for a in ("foot", "gedit")
                ),
                "zero erros de protocolo": "ProtocolError" not in log,
            })
        elif mode == "no-runtime":
            checks.update({
                "fallback imediato no-runtime": any(
                    "reason=no-runtime" in text for text in policies.values()
                ),
                "fallback < 100ms (sem latência no caminho frio)": fallback_ms is not None
                    and fallback_ms < 100,
                "sem frame suprimido": not suppressed,
                "zero erros de protocolo": "ProtocolError" not in log,
            })
        elif mode == "timeout":
            checks.update({
                "runtime conectou": "POLICY_TS READY" in rlog and "ready_reply=OK" in rlog,
                "runtime silencioso": "POLICY_TS NO_REPLY" in rlog,
                "timeout aplicado": any("reason=timeout" in text for text in policies.values()),
                "zero erros de protocolo": "ProtocolError" not in log,
            })
        else:
            checks.update({
                "regra editada em TS carregada": "POLICY_TS RULES" in rlog
                    and "source=edited-ts" in rlog,
                "aero-ts-new → none por regra TS": decisions.get("aero-ts-new", (None, "", ""))[1:] == ("none", "rule"),
                "compositor aplicou sem recompilar Rust": any(
                    "decision=none fallback=false" in text for text in policies.values()
                ),
                "frame suprimido": bool(suppressed),
                "zero erros de protocolo": "ProtocolError" not in log,
            })
        if mode == "no-runtime":
            print(f"fallback_ms={fallback_ms:.1f}" if fallback_ms is not None else "fallback_ms=None")
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {name}/{label}")
        return all(checks.values())
    finally:
        for client in clients:
            stop(client)
        stop(runtime)
        if comp is not None and comp.poll() is None:
            comp.terminate()
            comp.wait(timeout=5)


def main():
    results = {
        "policy": run_case("policy", "policy"),
        "no-runtime": run_case("no-runtime", "no-runtime"),
        "timeout": run_case("timeout", "timeout"),
        "edited-rules": run_case("edited-rules", "edited-rules"),
    }
    print("resultados=" + str(results))
    print("VEREDITO: " + ("OK" if all(results.values()) else "FALHA"))
    return 0 if all(results.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
