#!/usr/bin/env python3
"""Testes de protocolo aero_frame_v1 com socketpair privilegiado (sem PID/env)."""
import os, socket, subprocess, sys, time, pathlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
COMP = ROOT / "target/debug/aeroshell-compositor"
CLIENT = ROOT / "target/debug/aeroshell-frame-client"
ENV = os.environ.copy()
ENV["LD_LIBRARY_PATH"] = "/tmp/opencode/aptroot/usr/lib/x86_64-linux-gnu"


def start_compositor(name, extra_env=None, pass_fds=(), spawn_client=None):
    log = open(f"/tmp/{name}-comp.log", "wb")
    env = ENV.copy()
    env["RUST_LOG"] = "info"
    env["RUST_BACKTRACE"] = "1"
    if extra_env:
        env.update(extra_env)
    args = [str(COMP), "--backend", "winit", "--socket", name]
    if spawn_client:
        args += ["--command", spawn_client]
    proc = subprocess.Popen(
        args, env=env, stdout=log, stderr=subprocess.STDOUT,
        pass_fds=tuple(pass_fds),
    )
    for _ in range(80):
        if (pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp")) / name).exists():
            break
        time.sleep(0.05)
    time.sleep(1)
    return proc, log


def run_public_client(name, env_extra):
    env = ENV.copy()
    env.update({"WAYLAND_DISPLAY": name, **env_extra})
    return subprocess.run([str(CLIENT)], env=env, capture_output=True, text=True, timeout=20)


def run_privileged(name, env_extra):
    parent, child = socket.socketpair()
    os.set_inheritable(child.fileno(), True)
    cenv = ENV.copy()
    cenv["WAYLAND_SOCKET"] = str(child.fileno())
    cenv["RUST_LOG"] = "info"
    comp, log = start_compositor(name, {"WAYLAND_SOCKET": str(child.fileno())}, pass_fds=(child.fileno(),), spawn_client="zenity --info --text=JANELA-XDG")
    os.set_inheritable(parent.fileno(), True)
    penv = ENV.copy()
    penv["WAYLAND_SOCKET"] = str(parent.fileno())
    penv.update(env_extra)
    result = subprocess.run(
        [str(CLIENT)], env=penv, capture_output=True, text=True,
        pass_fds=(parent.fileno(),), timeout=20,
    )
    time.sleep(0.3)
    comp.terminate()
    try:
        comp.wait(timeout=5)
    except subprocess.TimeoutExpired:
        comp.kill()
    log.close()
    out = (result.stdout + result.stderr)
    text = pathlib.Path(f"/tmp/{name}-comp.log").read_text(errors="ignore")
    return result, out, text


def main():
    # (a) cliente do socket público, mesmo com AERO_SHELL_PID forjado, não vê o global.
    name = f"wayland-aero-frame-py-a-{os.getpid()}"
    comp, log = start_compositor(name)
    result = run_public_client(name, {"AERO_EXPECT_NO_GLOBAL": "1", "AERO_SHELL_PID": "1"})
    comp.terminate()
    comp.wait(timeout=5)
    log.close()
    assert result.returncode == 0, result.stderr
    assert "não é visível" in result.stdout + result.stderr, result.stdout + result.stderr
    print("OK (a): socket público nunca vê o global, mesmo com AERO_SHELL_PID forjado")

    # (b) cliente privilegiado via socketpair: segundo get_frame => already_constructed.
    name = f"wayland-aero-frame-py-b-{os.getpid()}"
    result, out, comp_out = run_privileged(name, {"AERO_TEST_DOUBLE": "1", "AERO_FRAME_WINDOW_ID": "0"})
    assert "already" in out.lower() or "construído" in out.lower() or result.returncode != 0, out
    assert "already_constructed" in comp_out.lower() or "já tem frame" in comp_out, comp_out[-2000:]
    print("OK (b): segundo get_frame gerou already_constructed")

    # (c) serial falso em move é ignorado silenciosamente.
    name = f"wayland-aero-frame-py-c-{os.getpid()}"
    result, out, comp_out = run_privileged(name, {"AERO_TEST_FALSE_SERIAL": "1", "AERO_FRAME_WINDOW_ID": "0"})
    assert "move do frame" not in comp_out, comp_out[-2000:]
    print("OK (c): move com serial falso ignorado")


if __name__ == "__main__":
    main()
