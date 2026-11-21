#!/usr/bin/env python3
"""02q: cold-start da decoração, do map ao primeiro render real.

Abre três `foot` (o inicial e dois concorrentes). Para cada janela nova mede o
timestamp ISO do `toplevel mapeado no space` até o primeiro RESIZE_GLUE, que é
o primeiro render do buffer WebKit real. Exige, antes desse buffer, um
FRAME_PLACEHOLDER sólido na mesma geometria-alvo.

Também verifica no sistema de processos que a âncora/pool/frames usam UM
WebKitWebProcess descendente do shell, e nos logs que o pool foi consumido e
reposto assincronamente até duas views prontas. Nenhuma amostra é descartada.
VM sem 3D: funcionamento e latência relativa apenas; não é benchmark de GPU.
"""
import atexit
import datetime
import os
import re
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/debug/aeroshell-compositor")
FRAME = os.path.join(ROOT, "target/debug/examples/frame_webview")
SOCK = f"wayland-aero-cold-{os.getpid()}"
LOG = f"/tmp/opencode/02q-cold-comp-{os.getpid()}.log"
LIMIT_MS = 300.0

for orphan in (f"/run/user/1000/{SOCK}",):
    if os.path.exists(orphan):
        os.unlink(orphan)

subprocess.run(
    ["cargo", "build", "-p", "aeroshell-compositor", "--features", "bench"],
    cwd=ROOT, check=True, stdout=subprocess.DEVNULL,
)
subprocess.run(
    ["cargo", "build", "-p", "aeroshell-shell", "--example", "frame_webview"],
    cwd=ROOT, check=True, stdout=subprocess.DEVNULL,
)

env = os.environ.copy()
for key in ("WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env["RUST_LOG"] = "debug"
log_file = open(LOG, "w")
comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK, "--command", "foot",
     "--shell", FRAME, "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
    env=env, stdout=log_file, stderr=subprocess.STDOUT,
)
clients = []


def cleanup():
    for client in clients:
        if client.poll() is None:
            client.terminate()
            try:
                client.wait(timeout=5)
            except subprocess.TimeoutExpired:
                client.kill()
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()
    log_file.close()


atexit.register(cleanup)


def log():
    with open(LOG, errors="ignore") as f:
        return re.sub(r"\x1b\[[0-9;]*m", "", f.read())


def first_real_times():
    found = {}
    for line in log().splitlines():
        m = re.search(r"RESIZE_GLUE t=(\d+) window_id=(\d+)", line)
        if m:
            found.setdefault(int(m.group(2)), int(m.group(1)))
    return found


def wait_for(label, predicate, timeout_s=20.0):
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        if predicate():
            return True
        if comp.poll() is not None:
            print(f"FALHA: compositor morreu esperando {label}", file=sys.stderr)
            return False
        time.sleep(0.02)
    print(f"FALHA: timeout esperando {label}", file=sys.stderr)
    return False


if not wait_for("primeiro render real da janela 0", lambda: 0 in first_real_times()):
    sys.exit(1)

# Abertura concorrente: duas novas WebViews seriam necessárias sem pool.
client_env = env.copy()
client_env["WAYLAND_DISPLAY"] = SOCK
for _ in range(2):
    clients.append(subprocess.Popen(
        ["foot"], env=client_env,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    ))

ready = lambda: all(window_id in first_real_times() for window_id in (1, 2))
wait_for("primeiro render real das janelas 1 e 2", ready)
time.sleep(3.0)  # permite observar a reposição assíncrona completa do pool
text = log()

map_times = {}
map_lines = {}
for line in text.splitlines():
    if "toplevel mapeado no space" not in line:
        continue
    m = re.match(r"(\d{4}-\d{2}-\d{2}T[^ ]+Z)", line)
    if not m:
        continue
    stamp = datetime.datetime.fromisoformat(m.group(1).replace("Z", "+00:00"))
    window_id = len(map_times)
    map_times[window_id] = int(stamp.timestamp() * 1_000_000_000)
    map_lines[window_id] = line

real_rows = {}
placeholder_rows = {}
for line in text.splitlines():
    m = re.search(
        r"RESIZE_GLUE t=(\d+) window_id=(\d+).*?target=(\d+)x(\d+)", line
    )
    if m:
        window_id = int(m.group(2))
        real_rows.setdefault(window_id, []).append({
            "t": int(m.group(1)), "target": (int(m.group(3)), int(m.group(4))),
            "raw": line,
        })
    m = re.search(
        r"FRAME_PLACEHOLDER t=(\d+) window_id=(\d+).*?w=(\d+) h=(\d+)", line
    )
    if m:
        window_id = int(m.group(2))
        placeholder_rows.setdefault(window_id, []).append({
            "t": int(m.group(1)), "size": (int(m.group(3)), int(m.group(4))),
            "raw": line,
        })

problems = []
print("--- cold-start (todas as primeiras amostras; cru) ---")
for window_id in (1, 2):
    map_ns = map_times.get(window_id)
    reals = real_rows.get(window_id, [])
    placeholders = placeholder_rows.get(window_id, [])
    if map_ns is None:
        problems.append(f"janela {window_id}: timestamp de map ausente")
    if not reals:
        problems.append(f"janela {window_id}: primeiro render real ausente")
    if map_ns is None or not reals:
        continue
    first_real = reals[0]
    delay_ms = (first_real["t"] - map_ns) / 1e6
    before = [p for p in placeholders if p["t"] < first_real["t"]]
    print(
        f"window_id={window_id} map_to_first_real_ms={delay_ms:.3f} "
        f"target={first_real['target']} placeholders_antes={len(before)}"
    )
    print("  MAP: " + map_lines[window_id])
    print("  REAL: " + first_real["raw"])
    if before:
        print("  PLACEHOLDER: " + before[0]["raw"])
        if before[0]["size"] != first_real["target"]:
            problems.append(
                f"janela {window_id}: placeholder {before[0]['size']} != "
                f"target {first_real['target']}"
            )
    else:
        problems.append(f"janela {window_id}: nenhum placeholder antes do buffer real")
    if delay_ms > LIMIT_MS:
        problems.append(
            f"janela {window_id}: {delay_ms:.3f} ms > {LIMIT_MS:.0f} ms"
        )

pool_takes = re.findall(r"FRAMEWEB: pool_take window_id=(\d+) available=(\d+)", text)
pool_ready = [int(v) for v in re.findall(r"FRAMEWEB: pool_ready available=(\d+)", text)]
anchor_alive = "FRAMEWEB: anchor viva" in text
print(f"pool_takes={pool_takes} pool_ready_sequencia={pool_ready} anchor_viva={anchor_alive}")
for window_id in (0, 1, 2):
    if not any(int(taken) == window_id for taken, _ in pool_takes):
        problems.append(f"pool não forneceu a janela {window_id}")
if not pool_ready or pool_ready[-1] != 2:
    problems.append(f"pool não terminou reposto com 2 views: {pool_ready}")
if not anchor_alive:
    problems.append("WebView-âncora viva não foi anunciada")

# Conta somente descendentes do frame_webview deste compositor, nunca processos
# WebKit de outras aplicações do usuário.
ps = subprocess.run(
    ["ps", "-eo", "pid=,ppid=,args="], check=True, text=True,
    stdout=subprocess.PIPE,
).stdout
processes = {}
children = {}
for line in ps.splitlines():
    fields = line.strip().split(None, 2)
    if len(fields) != 3:
        continue
    pid, ppid, args = int(fields[0]), int(fields[1]), fields[2]
    processes[pid] = (ppid, args)
    children.setdefault(ppid, []).append(pid)
shell_pids = [
    pid for pid, (ppid, args) in processes.items()
    if ppid == comp.pid and "frame_webview" in args
]
descendants = set()
queue = list(shell_pids)
while queue:
    pid = queue.pop()
    for child in children.get(pid, []):
        if child not in descendants:
            descendants.add(child)
            queue.append(child)
web_processes = sorted(
    pid for pid in descendants
    if os.path.basename(processes[pid][1].split(None, 1)[0]) == "WebKitWebProcess"
)
print(f"shell_pids={shell_pids} webkit_web_processes={web_processes}")
for pid in web_processes:
    print(f"  WEBPROCESS: {processes[pid][1]}")
if len(shell_pids) != 1:
    problems.append(f"esperado 1 shell descendente, encontrado {shell_pids}")
if len(web_processes) != 1:
    problems.append(
        f"related-view deveria manter 1 WebKitWebProcess, medido {len(web_processes)}"
    )

print(f"LOG_COMPOSITOR={LOG}")
if problems:
    print(f"FALHA: {len(problems)} problema(s)")
    for problem in problems:
        print("  " + problem)
    print("VEREDITO: FALHA")
    sys.exit(1)
print("VEREDITO: OK")
