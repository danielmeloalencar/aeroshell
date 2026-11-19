#!/usr/bin/env python3
"""Benchmark do protótipo do frame: release, 3 repeticoes, N=5 e N=20.

Rota única: o compositor (winit) lança o cliente de bench como shell privilegiado
via --shell (supervisor interno). O socketpair é criado dentro do compositor; os
logs do cliente saem no stdout do compositor (comp_log). Nenhum harness in-process.
VM sem 3D: valores relativos, nao comparaveis com GPU real.
"""
import os
import re
import socket
import statistics
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/release/aeroshell-compositor")
CLIENT = os.path.join(ROOT, "target/release/aeroshell-frame-client")

REPS = 3
NS = (5, 20)

# Os knobs de benchmark só existem com a feature `bench`: construir essa variante.
subprocess.run(
    ["cargo", "build", "--release",
     "-p", "aeroshell-compositor", "--features", "bench",
     "-p", "aeroshell-frame-client", "--features", "bench"],
    cwd=ROOT, check=True,
)

base = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    base.pop(key, None)


def read(path):
    return re.sub(r"\x1b\[[0-9;]*m", "", open(path, errors="ignore").read())


def wait_log(path, substr, timeout_s):
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if substr in read(path):
            return True
        time.sleep(0.1)
    return False


def wait_socket(sock_name, timeout_s=10):
    path = f"/run/user/1000/{sock_name}"
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if os.path.exists(path):
            return True
        time.sleep(0.05)
    return False


def start_comp(sock_name, comp_log, shell_env, extra_env=None, size=None):
    """Sobe o compositor com o cliente de bench como shell privilegiado."""
    cenv = base.copy()
    cenv.update(
        WAYLAND_DISPLAY=base.get("WAYLAND_DISPLAY", "wayland-0"),
        AERO_FRAME_BENCH_WINDOWS="1",
        RUST_LOG="warn",
    )
    if extra_env:
        cenv.update(extra_env)
    args = [COMP, "--backend", "winit", "--socket", sock_name]
    if size:
        args += ["--size", size]
    args += ["--shell", CLIENT]
    for kv in shell_env:
        args += ["--shell-env", kv]
    return subprocess.Popen(
        args, env=cenv, stdout=open(comp_log, "w"), stderr=subprocess.STDOUT,
    )


def stop_comp(comp):
    comp.terminate()
    try:
        comp.wait(timeout=5)
    except subprocess.TimeoutExpired:
        comp.kill()


def run_once(n, rep):
    tag = f"n{n}-r{rep}-{os.getpid()}"
    sock_name = f"wayland-aero-bench-{tag}"
    comp_log = f"/tmp/aero-bench-comp-{tag}.log"
    input_sock = f"/tmp/aero-bench-input-{tag}.sock"
    for p in (input_sock,):
        try:
            os.unlink(p)
        except FileNotFoundError:
            pass

    comp = start_comp(
        sock_name, comp_log,
        shell_env=[
            "AERO_FRAME_BENCH=1",
            "AERO_FRAME_WINDOW_ID=0",
            "AERO_WAIT_SOCKET=1",
            "AERO_FRAME_ACK_DELAY_MS=0",
        ],
        extra_env={"AERO_FRAME_STEPS": str(n), "AERO_TEST_INPUT_SOCK": input_sock},
        size="1280x800",
    )
    wait_socket(sock_name)
    # O cliente (filho do supervisor) imprime o resumo ao terminar os N passos.
    wait_log(comp_log, "saw_manager=", 30)

    # (iv) latencia do ponteiro: polling ate 3 leituras consecutivas com
    # variacao < 2%. Todas as leituras (inclusive as instaveis) sao registradas.
    pointer_all = []
    stable = None
    if os.path.exists(input_sock):
        for _ in range(12):
            s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            s.connect(input_sock)
            sent = time.time_ns()
            s.sendall(b"motion 10 10\n")
            s.close()
            time.sleep(0.12)
            m = re.findall(r"POINTER_MOTION serial=\d+ t=(\d+)", read(comp_log))
            if not m:
                continue
            pointer_all.append((int(m[-1]) - sent) / 1e6)
            last3 = pointer_all[-3:]
            if len(last3) == 3:
                spread = (max(last3) - min(last3)) / statistics.mean(last3)
                if spread < 0.02:
                    stable = last3
                    break
            time.sleep(0.04)
    pointer_ms = statistics.median(stable) if stable else None

    time.sleep(0.2)
    stop_comp(comp)

    log = read(comp_log)
    commits = [float(x) for x in re.findall(r"configure_to_commit_ms=([\d.]+)", log)]
    mismatch = len(re.findall(r"descompasso=true", log))
    timeouts = len(re.findall(r"FRAME_TIMEOUT", log))
    frames_ok = len(re.findall(r"descompasso=false", log))
    return dict(
        commits=commits,
        mismatch=mismatch,
        timeouts=timeouts,
        frames_ok=frames_ok,
        pointer_ms=pointer_ms,
        pointer_samples=pointer_all,
        comp_log=comp_log,
    )


def run_timeout_probe():
    """Prova que o contador de timeout (iii) dispara: ack demora 150 ms > 100 ms."""
    tag = f"timeout-{os.getpid()}"
    sock_name = f"wayland-aero-bench-{tag}"
    comp_log = f"/tmp/aero-bench-comp-{tag}.log"
    comp = start_comp(
        sock_name, comp_log,
        shell_env=[
            "AERO_FRAME_BENCH=1",
            "AERO_FRAME_WINDOW_ID=0",
            "AERO_FRAME_ACK_DELAY_MS=150",
        ],
        extra_env={"AERO_FRAME_STEPS": "1"},
    )
    wait_socket(sock_name)
    wait_log(comp_log, "saw_manager=", 30)
    time.sleep(0.4)
    stop_comp(comp)
    return len(re.findall(r"FRAME_TIMEOUT", read(comp_log)))


def run_mismatch_probe():
    """Prova que o detector (ii) acusa descompasso quando o conteúdo é menor."""
    tag = f"mismatch-{os.getpid()}"
    sock_name = f"wayland-aero-bench-{tag}"
    comp_log = f"/tmp/aero-bench-comp-{tag}.log"
    comp = start_comp(
        sock_name, comp_log,
        shell_env=[
            "AERO_FRAME_BENCH=1",
            "AERO_FRAME_WINDOW_ID=0",
            "AERO_FRAME_CONTENT_MISMATCH=1",
        ],
        extra_env={"AERO_FRAME_STEPS": "1"},
    )
    wait_socket(sock_name)
    wait_log(comp_log, "saw_manager=", 30)
    time.sleep(0.3)
    stop_comp(comp)
    return len(re.findall(r"descompasso=true", read(comp_log)))


print("=== benchmark frame (release, VM sem 3D, valores relativos) ===")
for n in NS:
    all_commits = []
    mismatches = timeouts = frames_ok = 0
    pointers = []
    for rep in range(1, REPS + 1):
        r = run_once(n, rep)
        all_commits += r["commits"]
        mismatches += r["mismatch"]
        timeouts += r["timeouts"]
        frames_ok += r["frames_ok"]
        if r["pointer_ms"] is not None:
            pointers.append(r["pointer_ms"])
        print(
            f"  N={n:2} rep={rep}: amostras={len(r['commits']):2} "
            f"descompasso={r['mismatch']} timeouts={r['timeouts']} "
            f"ponteiro_ms={r['pointer_ms']} ponteiro_amostras="
            f"{[round(x, 3) for x in r['pointer_samples']]}"
        )
    mean = statistics.mean(all_commits) if all_commits else float("nan")
    p95 = (
        sorted(all_commits)[round(0.95 * (len(all_commits) - 1))]
        if all_commits
        else float("nan")
    )
    print(
        f"  N={n:2} TOTAL: amostras={len(all_commits)} mean_ms={mean:.3f} "
        f"p95_ms={p95:.3f} descompasso={mismatches} timeouts={timeouts} "
        f"frames_ok={frames_ok} ponteiro_mean_ms="
        f"{(statistics.mean(pointers) if pointers else float('nan')):.3f}"
    )

timeouts_forced = run_timeout_probe()
print(f"=== verificacao (iii): ack com 150 ms => FRAME_TIMEOUT={timeouts_forced} ===")
mismatch_forced = run_mismatch_probe()
print(f"=== verificacao (ii): conteudo menor => descompasso=true em {mismatch_forced} commit(s) ===")