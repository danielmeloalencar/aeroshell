#!/usr/bin/env python3
"""Benchmark do protótipo do frame: release, 3 repeticoes, N=5 e N=20.

Fluxo real de producao: compositor headless com AERO_PRIVILEGED_FD e cliente
privilegiado com WAYLAND_SOCKET (socketpair). Nenhum harness in-process.
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


def run_once(n, rep):
    tag = f"n{n}-r{rep}-{os.getpid()}"
    sock_name = f"wayland-aero-bench-{tag}"
    comp_log = f"/tmp/aero-bench-comp-{tag}.log"
    client_log = f"/tmp/aero-bench-client-{tag}.log"
    input_sock = f"/tmp/aero-bench-input-{tag}.sock"
    for p in (input_sock,):
        try:
            os.unlink(p)
        except FileNotFoundError:
            pass

    a, b = socket.socketpair()
    cfd, sfd = a.fileno(), b.fileno()
    cenv = base.copy()
    cenv.update(
        AERO_PRIVILEGED_FD=str(cfd),
        AERO_FRAME_STEPS=str(n),
        AERO_FRAME_BENCH_WINDOWS="1",
        AERO_TEST_INPUT_SOCK=input_sock,
        RUST_LOG="warn",
    )
    comp = subprocess.Popen(
        [COMP, "--backend", "winit", "--socket", sock_name, "--size", "1280x800"],
        env=cenv,
        pass_fds=(cfd,),
        stdout=open(comp_log, "w"),
        stderr=subprocess.STDOUT,
    )
    a.close()
    sock_path = f"/run/user/1000/{sock_name}"
    for _ in range(100):
        if os.path.exists(sock_path):
            break
        time.sleep(0.05)

    senv = base.copy()
    senv.update(
        WAYLAND_SOCKET=str(sfd),
        WAYLAND_DISPLAY=sock_name,
        AERO_FRAME_BENCH="1",
        AERO_FRAME_WINDOW_ID="0",
        AERO_WAIT_SOCKET="1",
        AERO_FRAME_ACK_DELAY_MS="0",
    )
    cli = subprocess.Popen(
        [CLIENT],
        env=senv,
        pass_fds=(sfd,),
        stdout=open(client_log, "w"),
        stderr=subprocess.STDOUT,
    )
    b.close()
    try:
        cli.wait(timeout=30)
    except subprocess.TimeoutExpired:
        cli.kill()

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
            clog = open(comp_log, errors="ignore").read()
            m = re.findall(r"POINTER_MOTION serial=\d+ t=(\d+)", clog)
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
    comp.terminate()
    try:
        comp.wait(timeout=5)
    except subprocess.TimeoutExpired:
        comp.kill()

    log = open(comp_log, errors="ignore").read()
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
    a, b = socket.socketpair()
    cfd, sfd = a.fileno(), b.fileno()
    cenv = base.copy()
    cenv.update(
        AERO_PRIVILEGED_FD=str(cfd),
        AERO_FRAME_STEPS="1",
        AERO_FRAME_BENCH_WINDOWS="1",
        RUST_LOG="warn",
    )
    comp = subprocess.Popen(
        [COMP, "--backend", "winit", "--socket", sock_name],
        env=cenv,
        pass_fds=(cfd,),
        stdout=open(comp_log, "w"),
        stderr=subprocess.STDOUT,
    )
    a.close()
    for _ in range(100):
        if os.path.exists(f"/run/user/1000/{sock_name}"):
            break
        time.sleep(0.05)
    senv = base.copy()
    senv.update(
        WAYLAND_SOCKET=str(sfd),
        WAYLAND_DISPLAY=sock_name,
        AERO_FRAME_BENCH="1",
        AERO_FRAME_WINDOW_ID="0",
        AERO_FRAME_ACK_DELAY_MS="150",
    )
    cli = subprocess.Popen([CLIENT], env=senv, pass_fds=(sfd,),
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    b.close()
    cli.wait(timeout=30)
    time.sleep(0.4)
    comp.terminate()
    try:
        comp.wait(timeout=5)
    except subprocess.TimeoutExpired:
        comp.kill()
    log = open(comp_log, errors="ignore").read()
    return len(re.findall(r"FRAME_TIMEOUT", log))


def run_mismatch_probe():
    """Prova que o detector (ii) acusa descompasso quando o conteúdo é menor."""
    tag = f"mismatch-{os.getpid()}"
    sock_name = f"wayland-aero-bench-{tag}"
    comp_log = f"/tmp/aero-bench-comp-{tag}.log"
    a, b = socket.socketpair()
    cfd, sfd = a.fileno(), b.fileno()
    cenv = base.copy()
    cenv.update(
        AERO_PRIVILEGED_FD=str(cfd),
        AERO_FRAME_STEPS="1",
        AERO_FRAME_BENCH_WINDOWS="1",
        RUST_LOG="warn",
    )
    comp = subprocess.Popen(
        [COMP, "--backend", "winit", "--socket", sock_name],
        env=cenv,
        pass_fds=(cfd,),
        stdout=open(comp_log, "w"),
        stderr=subprocess.STDOUT,
    )
    a.close()
    for _ in range(100):
        if os.path.exists(f"/run/user/1000/{sock_name}"):
            break
        time.sleep(0.05)
    senv = base.copy()
    senv.update(
        WAYLAND_SOCKET=str(sfd),
        WAYLAND_DISPLAY=sock_name,
        AERO_FRAME_BENCH="1",
        AERO_FRAME_WINDOW_ID="0",
        AERO_FRAME_CONTENT_MISMATCH="1",
    )
    cli = subprocess.Popen([CLIENT], env=senv, pass_fds=(sfd,),
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    b.close()
    cli.wait(timeout=30)
    time.sleep(0.3)
    comp.terminate()
    try:
        comp.wait(timeout=5)
    except subprocess.TimeoutExpired:
        comp.kill()
    log = open(comp_log, errors="ignore").read()
    return len(re.findall(r"descompasso=true", log))


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
