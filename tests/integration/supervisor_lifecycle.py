#!/usr/bin/env python3
"""Supervisor interno: privilégio pela ponta do socketpair + fail-secure de sessão.

Fase 1 (privilégio): o shell lançado pelo supervisor (--shell) vê o
aero_frame_manager_v1; um cliente PÚBLICO no socket público não vê.
Fase 2 (ciclo de vida): kill -9 no compositor mata o filho privilegiado via
PDEATHSIG — garantia explícita do kernel, não o comportamento incidental do GDK.
Fase 3 (ciclo de vida): o mesmo para o cliente PÚBLICO lançado via --command.
"""
import os
import re
import signal
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/debug/aeroshell-compositor")
CLIENT = os.path.join(ROOT, "target/debug/aeroshell-frame-client")
RUNTIME = os.environ.get("XDG_RUNTIME_DIR", "/tmp")

base = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    base.pop(key, None)


def strip_ansi(txt):
    return re.sub(r"\x1b\[[0-9;]*m", "", txt)


def read(path):
    return strip_ansi(open(path, errors="ignore").read())


def wait_log(path, substr, timeout_s):
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if substr in read(path):
            return True
        time.sleep(0.1)
    return False


def wait_socket(sock, timeout_s):
    path = os.path.join(RUNTIME, sock)
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if os.path.exists(path):
            return path
        time.sleep(0.05)
    return None


def proc_alive(pid):
    return os.path.exists(f"/proc/{pid}")


def cleanup(comp):
    if comp.poll() is None:
        comp.kill()
    try:
        comp.wait(timeout=5)
    except subprocess.TimeoutExpired:
        pass


failures = []

# --- Fase 1: privilégio -----------------------------------------------------
SOCK1 = f"wayland-aero-sup-priv-{os.getpid()}"
LOG1 = f"/tmp/aero-sup-priv-{os.getpid()}.log"
for orphan in (os.path.join(RUNTIME, SOCK1),):
    if os.path.exists(orphan):
        os.unlink(orphan)

env1 = base.copy()
env1.update(RUST_LOG="info", AERO_ONLY_BIND="1", WAYLAND_DISPLAY=base.get("WAYLAND_DISPLAY", "wayland-0"))
comp1 = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK1, "--shell", CLIENT],
    env=env1, stdout=open(LOG1, "w"), stderr=subprocess.STDOUT,
)
if not wait_socket(SOCK1, 10):
    print("FALHA: compositor não criou o socket público")
    cleanup(comp1)
    sys.exit(1)

# O shell privilegiado (filho do supervisor) herda o stdout do compositor.
if not wait_log(LOG1, "bind manager global=", 10):
    failures.append("shell privilegiado NÃO viu aero_frame_manager_v1")
else:
    print("OK privilégio: shell do supervisor viu o manager (ponta privada)")

# Cliente PÚBLICO no socket público: não pode ver o manager.
pub_env = base.copy()
pub_env.update(WAYLAND_DISPLAY=SOCK1, AERO_EXPECT_NO_GLOBAL="1")
pub_env.pop("WAYLAND_SOCKET", None)
pub = subprocess.run([CLIENT], env=pub_env, capture_output=True, text=True, timeout=15)
if "não é visível para cliente comum" in pub.stderr and pub.returncode == 0:
    print("OK privilégio: cliente público NÃO viu o manager")
else:
    failures.append(f"cliente público viu o manager ou falhou: rc={pub.returncode} {pub.stderr!r}")
cleanup(comp1)

# --- Fase 2: ciclo de vida (kill -9 -> PDEATHSIG) ---------------------------
SOCK2 = f"wayland-aero-sup-life-{os.getpid()}"
LOG2 = f"/tmp/aero-sup-life-{os.getpid()}.log"
for orphan in (os.path.join(RUNTIME, SOCK2),):
    if os.path.exists(orphan):
        os.unlink(orphan)

env2 = base.copy()
env2.update(RUST_LOG="info", WAYLAND_DISPLAY=base.get("WAYLAND_DISPLAY", "wayland-0"))
comp2 = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK2,
     "--shell", "/bin/sleep", "--shell-arg", "60"],
    env=env2, stdout=open(LOG2, "w"), stderr=subprocess.STDOUT,
)
if not wait_socket(SOCK2, 10) or not wait_log(LOG2, "shell privilegiado lançado", 10):
    print("FALHA: supervisor não lançou o shell")
    cleanup(comp2)
    sys.exit(1)

m = re.search(r"shell privilegiado lançado[^\n]*?pid=(\d+)", read(LOG2))
if not m:
    print("FALHA: não identifiquei o pid do filho no log")
    cleanup(comp2)
    sys.exit(1)
child_pid = int(m.group(1))
time.sleep(0.3)
if not proc_alive(child_pid):
    failures.append(f"filho {child_pid} não está vivo após o spawn")
else:
    ppid = int(open(f"/proc/{child_pid}/stat").read().split()[3])
    if ppid != comp2.pid:
        failures.append(f"filho {child_pid} não é filho do compositor (ppid={ppid})")
    else:
        print(f"OK lifecycle: filho sleep pid={child_pid} é filho do compositor pid={comp2.pid}")

# Morte abrupta do compositor: o kernel deve matar o filho (PDEATHSIG=SIGKILL).
os.kill(comp2.pid, signal.SIGKILL)
comp2.wait(timeout=5)
deadline = time.time() + 5
while time.time() < deadline and proc_alive(child_pid):
    time.sleep(0.1)
if proc_alive(child_pid):
    failures.append(f"filho {child_pid} SOBREVIVEU ao kill -9 do compositor (fail-secure quebrado)")
    try:
        os.kill(child_pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
else:
    print(f"OK lifecycle: kill -9 no compositor matou o filho {child_pid} (PDEATHSIG)")

# --- Fase 3: ciclo de vida do cliente PÚBLICO (--command) ------------------
# O filho --command também é rastreado pelo supervisor: kill -9 no compositor
# deve matá-lo (PDEATHSIG), não orphanar um cliente público na sessão.
SOCK3 = f"wayland-aero-sup-cmd-{os.getpid()}"
LOG3 = f"/tmp/aero-sup-cmd-{os.getpid()}.log"
for orphan in (os.path.join(RUNTIME, SOCK3),):
    if os.path.exists(orphan):
        os.unlink(orphan)

env3 = base.copy()
env3.update(RUST_LOG="info", WAYLAND_DISPLAY=base.get("WAYLAND_DISPLAY", "wayland-0"))
comp3 = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK3,
     "--command", "exec /bin/sleep 60"],
    env=env3, stdout=open(LOG3, "w"), stderr=subprocess.STDOUT,
)
if not wait_socket(SOCK3, 10) or not wait_log(LOG3, "cliente inicial iniciado", 10):
    print("FALHA: supervisor não lançou o cliente --command")
    cleanup(comp3)
    sys.exit(1)

m = re.search(r"cliente inicial iniciado[^\n]*?pid=(\d+)", read(LOG3))
if not m:
    print("FALHA: não identifiquei o pid do cliente --command no log")
    cleanup(comp3)
    sys.exit(1)
cmd_pid = int(m.group(1))
time.sleep(0.3)
if not proc_alive(cmd_pid):
    failures.append(f"cliente --command {cmd_pid} não está vivo após o spawn")
else:
    ppid = int(open(f"/proc/{cmd_pid}/stat").read().split()[3])
    if ppid != comp3.pid:
        failures.append(f"cliente --command {cmd_pid} não é filho do compositor (ppid={ppid})")
    else:
        print(f"OK lifecycle: cliente --command pid={cmd_pid} é filho do compositor pid={comp3.pid}")

os.kill(comp3.pid, signal.SIGKILL)
comp3.wait(timeout=5)
deadline = time.time() + 5
while time.time() < deadline and proc_alive(cmd_pid):
    time.sleep(0.1)
if proc_alive(cmd_pid):
    failures.append(f"cliente --command {cmd_pid} SOBREVIVEU ao kill -9 do compositor (orphan)")
    try:
        os.kill(cmd_pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
else:
    print(f"OK lifecycle: kill -9 no compositor matou o cliente --command {cmd_pid} (PDEATHSIG)")

print(f"logs: {LOG1} {LOG2} {LOG3}")
if failures:
    for f in failures:
        print(f"FALHA: {f}")
    sys.exit(1)
print("SUPERVISOR: PASSOU (privilégio pela ponta privada + fail-secure de sessão)")