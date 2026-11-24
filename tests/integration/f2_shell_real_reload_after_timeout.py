#!/usr/bin/env python3
"""F2-shell-real: teste-first do reload DEPOIS de um timeout de política.

Baseline: `policy-reload` em window_ipc.rs só reconsulta `decorations.keys()`.
Quando uma decisão expira (`reason=timeout`), `policy_fallback` NÃO guarda a
decisão — a janela sai do mapa e NUNCA mais é reconsultada nos reloads
seguintes. No juiz visual isso apareceu como "mudei a política e um foot ficou
sem decoração" (o outro mudava normalmente).

Medição: parar o runtime (SIGSTOP) durante um reload para forçar o timeout das
duas janelas, voltar o runtime (SIGCONT), fazer outro reload e exigir que
AMBAS recebam decisão explícita de novo.
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
RUNTIME_DIR = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-reload-timeout-"))


def wait_for(predicate, timeout=15):
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
        'const config: DecorationPolicyConfig = { default: "Server", rules: [\n'
        '  { app_id: "aero-test-a", class: null, policy: "' + policy + '" },\n'
        '  { app_id: "aero-test-b", class: null, policy: "' + policy + '" },\n'
        '] };\nexport default config;\n'
    )


def main():
    config = OUT / "config"
    (config / "aeroshell").mkdir(parents=True, exist_ok=True)
    rules = config / "aeroshell" / "rules.ts"
    write_rules(rules, "Server")
    sock = f"wayland-aero-timeout-{os.getpid()}"
    ipc = OUT / "ipc.sock"
    comp_log = OUT / "comp.log"
    runtime_log = OUT / "runtime.log"
    env = {**os.environ, "RUST_LOG": "info", "AERO_WINDOW_IPC_SOCK": str(ipc),
           "AERO_POLICY_TIMEOUT_MS": "300", "XDG_CONFIG_HOME": str(config), "HOME": str(OUT)}
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD", "AERO_DECORATION_RULES"):
        env.pop(key, None)

    comp = runtime = None
    clients = []
    try:
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
             "--socket", sock, "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, env=env, stdout=comp_log.open("wb"), stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        if not wait_for(lambda: ipc.exists() and (RUNTIME_DIR / sock).exists(), 15):
            print("FALHA: compositor não subiu", file=sys.stderr)
            return 1
        # 1º runtime: anuncia policy-ready mas NÃO responde — força fallback
        # por timeout nas duas janelas (determinístico, sem corrida de SIGSTOP).
        runtime = subprocess.Popen(
            ["bun", str(ROOT / "packages/runtime/src/policy.ts")], cwd=ROOT,
            env={**env, "AERO_WINDOW_IPC_SOCK": str(ipc), "AERO_POLICY_NO_REPLY": "1"},
            stdout=runtime_log.open("wb"), stderr=subprocess.STDOUT, start_new_session=True,
        )
        wait_for(lambda: "POLICY_TS NO_REPLY=1" in runtime_log.read_text(errors="ignore"), 15)
        for app in ("aero-test-a", "aero-test-b"):
            clients.append(subprocess.Popen(
                [str(ROOT / "target/debug/examples/policy_client"), app], cwd=ROOT,
                stdout=subprocess.DEVNULL, stderr=(OUT / f"{app}.err").open("wb"),
                env={**os.environ, "WAYLAND_DISPLAY": sock}, start_new_session=True,
            ))
        waited = wait_for(
            lambda: len(re.findall(
                r"POLICY t=\d+ window_id=\d+ decision=fallback fallback=true reason=timeout",
                comp_log.read_text(errors="ignore"),
            )) >= 2,
            15,
        )
        timed_out = set(re.findall(
            r"POLICY t=\d+ window_id=(\d+) decision=fallback fallback=true reason=timeout",
            comp_log.read_text(errors="ignore"),
        ))
        print(f"janelas em timeout: {sorted(timed_out)} (esperado 2: {waited})")
        if not waited:
            print("FALHA: não consegui forçar timeout", file=sys.stderr)
            return 1
        baseline = set(re.findall(
            r"POLICY_TS REQUEST id=(\d+) app_id=aero-test-", runtime_log.read_text(errors="ignore")
        ))

        # 2º runtime: responde normalmente. Um reload nas janelas cujas decisões
        # expiraram tem de acontecer — baseline NÃO reconsulta (decorations vazia).
        stop(runtime)
        runtime = subprocess.Popen(
            ["bun", str(ROOT / "packages/runtime/src/policy.ts")], cwd=ROOT,
            env={**env, "AERO_WINDOW_IPC_SOCK": str(ipc)},
            stdout=runtime_log.open("wb"), stderr=subprocess.STDOUT, start_new_session=True,
        )
        wait_for(lambda: "POLICY_TS READY" in runtime_log.read_text(errors="ignore"), 15)
        before = len(re.findall(r"POLICY_TS DECISION id=", runtime_log.read_text(errors="ignore")))
        rules.write_text(rules.read_text().replace('"Server"', '"None"', 1))
        wait_for(
            lambda: len(re.findall(r"POLICY_TS DECISION id=", runtime_log.read_text(errors="ignore"))) >= before + 2,
            10,
        )
        time.sleep(0.5)
        decided_again = set(re.findall(
            r"POLICY_TS DECISION id=(\d+) app_id=aero-test-", runtime_log.read_text(errors="ignore")
        ))
        rlog = runtime_log.read_text(errors="ignore")
        decided_again = set(re.findall(r"POLICY_TS DECISION id=(\d+) app_id=aero-test-", rlog))
        print("=== runtime (cru) ===")
        for line in rlog.splitlines():
            if "POLICY_TS" in line:
                print(line[:180])
        print("=== compositor (cru) ===")
        for line in comp_log.read_text(errors="ignore").splitlines():
            if "POLICY t=" in line:
                print(line[:180])

        checks = {
            "timeout forçado nas 2 janelas": len(timed_out) == 2,
            "reload reconsulta as 2 janelas": decided_again == baseline,
        }
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {label}")
        print("VEREDITO: " + ("OK" if all(checks.values()) else "FALHA"))
        return 0 if all(checks.values()) else 1
    finally:
        if runtime is not None and runtime.poll() is None:
            try:
                os.kill(runtime.pid, signal.SIGCONT)
            except ProcessLookupError:
                pass
        for proc in [*clients, runtime]:
            stop(proc)
        stop(comp)


if __name__ == "__main__":
    sys.exit(main())
