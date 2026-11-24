#!/usr/bin/env python3
"""F2-decoration-policy: teste-first do mecanismo de política de decoração.

Baseline (sem mecanismo): `policy-ready`/`policy` não existem, nenhum evento
`DecorationRequest` e nenhuma sonda `POLICY` → VEREDITO: FALHA.

Pre-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-compositor --example policy_client
  cargo build -p aeroshell-compositor --example policy_runtime_mock
  cargo build -p aeroshell-shell --example frame_webview

Uso: python3 tests/integration/f2_decoration_policy.py
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
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-policy-"))


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
    """mode: 'policy' (mock responde), 'no-runtime', 'timeout'."""
    sock = f"wayland-aero-f2-policy-{name}-{os.getpid()}"
    dir_ = OUT / name
    dir_.mkdir(parents=True, exist_ok=True)
    ipc = str(dir_ / "ipc.sock")
    comp_log = dir_ / "comp.log"
    client_err = dir_ / "client.err"
    mock_err = dir_ / "mock.err"
    env = {**os.environ, "RUST_LOG": "debug", "AERO_WINDOW_IPC_SOCK": ipc}
    if mode == "timeout":
        env["AERO_POLICY_TIMEOUT_MS"] = "200"
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET"):
        env.pop(key, None)
    comp = client = mock = None
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
            mock_env = {**os.environ, "AERO_WINDOW_IPC_SOCK": ipc}
            if mode == "timeout":
                mock_env["AERO_POLICY_IGNORE"] = "1"
            mock = subprocess.Popen(
                [str(ROOT / "target/debug/examples/policy_runtime_mock")],
                cwd=ROOT, stdout=subprocess.DEVNULL, stderr=mock_err.open("wb"),
                env=mock_env, start_new_session=True,
            )
            wait_for(lambda: "POLICY_MOCK READY" in mock_err.read_text(errors="replace"))
        client = subprocess.Popen(
            [str(ROOT / "target/debug/examples/policy_client"), "aero-no-deco"],
            cwd=ROOT, stdout=subprocess.DEVNULL, stderr=client_err.open("wb"),
            env={**os.environ, "WAYLAND_DISPLAY": sock}, start_new_session=True,
        )
        ready = wait_for(lambda: "POLICY_CLIENT READY" in client_err.read_text(errors="replace"))
        ready_at = time.time()
        fallback_ms = None
        deadline = time.time() + 2
        while time.time() < deadline and fallback_ms is None:
            if "reason=no-runtime" in comp_log.read_text(errors="replace"):
                fallback_ms = (time.time() - ready_at) * 1000
            time.sleep(0.01)
        time.sleep(1)

        log = comp_log.read_text(errors="replace")
        cout = client_err.read_text(errors="replace")
        mout = mock_err.read_text(errors="replace") if mock is not None else ""
        print(f"=== {name}: compositor (POLICY cru) ===")
        for line in log.splitlines():
            if "POLICY" in line or "DecorationRequest" in line:
                print(line)
        print(f"=== {name}: mock ===")
        print(mout, end="")

        policies = re.findall(r"POLICY t=\d+ window_id=(\d+) ([^\n]+)", log)
        renders = re.findall(r"POLICY_RENDER t=\d+ window_id=(\d+) suppressed=(\w+)", log)
        checks = {
            "cliente pronto": ready,
        }
        if mode == "policy":
            checks.update({
                "runtime anunciou policy-ready": "POLICY_MOCK READY" in mout
                    and "ready_reply=OK" in mout,
                "pedido de decisão no stream": "POLICY_MOCK REQUEST" in mout
                    and "kind=decoration_request" in log,
                "decisão none aplicada": any("decision=none fallback=false" in text for _, text in policies),
                "frame suprimido pela política": any(sup == "policy_none" for _, sup in renders),
                "zero erros de protocolo": "ProtocolError" not in log,
            })
        elif mode == "no-runtime":
            fb = [text for _, text in policies if "reason=no-runtime" in text]
            checks.update({
                "fallback imediato no-runtime": bool(fb),
                "fallback < 100ms (sem latência no caminho frio)": fallback_ms is not None
                    and fallback_ms < 100,
                "sem frame suprimido": not any(sup == "policy_none" for _, sup in renders),
                "zero erros de protocolo": "ProtocolError" not in log,
            })
        else:
            checks.update({
                "timeout com runtime lento": any("reason=timeout" in text for _, text in policies),
                "zero erros de protocolo": "ProtocolError" not in log,
            })
        if mode == "no-runtime":
            print(f"fallback_ms={fallback_ms:.1f}" if fallback_ms is not None else "fallback_ms=None")
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {name}/{label}")
        return all(checks.values())
    finally:
        stop(mock)
        stop(client)
        if comp is not None and comp.poll() is None:
            comp.terminate()
            comp.wait(timeout=5)


def main():
    results = {
        "policy": run_case("policy", "policy"),
        "no-runtime": run_case("no-runtime", "no-runtime"),
        "timeout": run_case("timeout", "timeout"),
    }
    print("resultados=" + str(results))
    print("VEREDITO: " + ("OK" if all(results.values()) else "FALHA"))
    return 0 if all(results.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
