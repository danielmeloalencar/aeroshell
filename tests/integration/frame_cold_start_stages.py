#!/usr/bin/env python3
"""02q (reaberta): marcos M0..M12 do map à decoração, incluindo a janela 0.

M0  init                 compositor "iniciando AeroShell"        (ISO)
M1  shell_spawn          "shell privilegiado lançado"            (ISO)
M2  webprocess           FRAMEWEB anchor_viva t=                 (ns)
M3  pool_ready           FRAMEWEB pool_ready t=                  (ns)
M4  associate            FRAMEWEB associate t=                   (ns)
M5  client_spawn         "cliente inicial iniciado"              (ISO)
M6  client_connect       "cliente Wayland conectado"             (ISO)
M7  map                  "toplevel mapeado no space"             (ISO)
M8  placeholder_pixel    FRAME_PLACEHOLDER t=                    (ns)
M9  html_finished        FRAMEWEB html_ready t=                  (ns)
M10 painted              FRAMEWEB html_painted t= / FRAME_PAINTED (ns)
M11 latch                RESIZE_GLUE t=                          (ns)
M12 presented            OUTPUT_FRAME t=                         (ns)

M11 é o primeiro render com buffer WebKit importado; M12 é a apresentação do
output. São números distintos: latch NÃO é presented. A janela 0 é medida como
as outras; o primeiro HTML frio é reportado à parte, sem limite inventado.
VM sem 3D: funcionamento e latência relativa, não benchmark de GPU.
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
SOCK = f"wayland-aero-coldstages-{os.getpid()}"
OUTDIR = "/tmp/opencode/02q-reopen"
LOG = f"{OUTDIR}/cold-comp-{os.getpid()}.log"
LIMIT_PLACEHOLDER_MS = 100.0
LIMIT_HOT_HTML_MS = 300.0
LIMIT_SPAWN_MS = 100.0

os.makedirs(OUTDIR, exist_ok=True)
orphan = f"/run/user/{os.getuid()}/{SOCK}"
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


def text():
    with open(LOG, errors="ignore") as f:
        return re.sub(r"\x1b\[[0-9;]*m", "", f.read())


def iso_ns(line):
    m = re.match(r"(\d{4}-\d{2}-\d{2}T[^ ]+Z)", line)
    if not m:
        return None
    try:
        stamp = datetime.datetime.fromisoformat(m.group(1).replace("Z", "+00:00"))
    except ValueError:
        return None
    return int(stamp.timestamp() * 1_000_000_000)


def first(pattern):
    """Primeiro valor de um marcador, com timestamp ns."""
    m = re.search(pattern, text())
    return int(m.group(1)) if m else None


def wait_for(label, predicate, timeout_s=25.0):
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


def real_times():
    found = {}
    for line in text().splitlines():
        m = re.search(r"RESIZE_GLUE t=(\d+) window_id=(\d+)", line)
        if m:
            found.setdefault(int(m.group(2)), int(m.group(1)))
    return found


if not wait_for("1º render real da janela 0", lambda: 0 in real_times()):
    sys.exit(1)

client_env = env.copy()
client_env["WAYLAND_DISPLAY"] = SOCK
for _ in range(2):
    clients.append(subprocess.Popen(
        ["foot"], env=client_env,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    ))

ready = lambda: all(w in real_times() for w in (1, 2))
wait_for("1º render real das janelas 1 e 2", ready)
time.sleep(3.0)
body = text()

# --- marcos -------------------------------------------------------------
ns = {}
for label, pattern in (
    ("M0_init", r"iniciando AeroShell"),
    ("M1_shell_spawn", r"shell privilegiado lançado pelo supervisor"),
    ("M5_client_spawn", r"cliente inicial iniciado"),
    ("M6_client_connect", r"cliente Wayland conectado ao socket público"),
    ("M2_webprocess", r"anchor_viva t=(\d+)"),
    ("M3_pool_ready", r"pool_ready available=\d+ t=(\d+)"),
    ("M4_associate", r"associate\(window_id=\d+\).* t=(\d+)"),
    ("M9_html_finished", r"html_ready t=(\d+)"),
    ("M10_painted", r"html_painted t=(\d+)"),
    ("M12_presented", r"OUTPUT_FRAME t=(\d+)"),
):
    for line in body.splitlines():
        if label.startswith(("M0_", "M1_", "M5_", "M6_")):
            if pattern in line and iso_ns(line) is not None:
                ns[label] = iso_ns(line)
                break
        else:
            m = re.search(pattern, line)
            if m:
                ns[label] = int(m.group(1))
                break

map_times, map_lines = {}, {}
for line in body.splitlines():
    if "toplevel mapeado no space" not in line:
        continue
    stamp = iso_ns(line)
    if stamp is None:
        continue
    window_id = len(map_times)
    map_times[window_id] = stamp
    map_lines[window_id] = line

output_frames = [int(m.group(1)) for line in body.splitlines()
                 if (m := re.search(r"OUTPUT_FRAME t=(\d+)", line))]

real_rows, placeholder_rows = {}, {}
for line in body.splitlines():
    m = re.search(r"RESIZE_GLUE t=(\d+) window_id=(\d+).*?target=(\d+)x(\d+)", line)
    if m:
        real_rows.setdefault(int(m.group(2)), []).append({
            "t": int(m.group(1)), "target": (int(m.group(3)), int(m.group(4))), "raw": line,
        })
    m = re.search(r"FRAME_PLACEHOLDER t=(\d+) window_id=(\d+).*?w=(\d+) h=(\d+)", line)
    if m:
        placeholder_rows.setdefault(int(m.group(2)), []).append({
            "t": int(m.group(1)), "size": (int(m.group(3)), int(m.group(4))), "raw": line,
        })

problems = []
print("--- marcos M0..M12 (cru) ---")
for label in ("M0_init", "M1_shell_spawn", "M2_webprocess", "M3_pool_ready",
              "M4_associate", "M5_client_spawn", "M6_client_connect",
              "M9_html_finished", "M10_painted", "M12_presented"):
    print(f"  {label}: {ns.get(label, 'AUSENTE')}")
if "M0_init" in ns and "M1_shell_spawn" in ns:
    print(f"  init→shell_spawn_ms={(ns['M1_shell_spawn'] - ns['M0_init']) / 1e6:.3f}")
if "M1_shell_spawn" in ns and "M5_client_spawn" in ns:
    spawn_ms = (ns["M5_client_spawn"] - ns["M1_shell_spawn"]) / 1e6
    print(f"  shell_spawn→client_spawn_ms={spawn_ms:.3f}")
    if spawn_ms > LIMIT_SPAWN_MS:
        problems.append(f"cliente nasceu {spawn_ms:.1f} ms depois do shell (limite {LIMIT_SPAWN_MS:.0f})")
else:
    problems.append("marcos M1 ou M5 ausentes (não dá para medir a serialização)")

for label in ("M2_webprocess", "M3_pool_ready", "M4_associate", "M9_html_finished",
              "M10_painted", "M12_presented"):
    if label not in ns:
        problems.append(f"marco {label} ausente sob bench")

print("--- cold-start por janela (todas as primeiras amostras; cru) ---")
for window_id in (0, 1, 2):
    map_ns = map_times.get(window_id)
    reals = real_rows.get(window_id, [])
    placeholders = placeholder_rows.get(window_id, [])
    if map_ns is None:
        problems.append(f"janela {window_id}: timestamp de map ausente")
        continue
    if not reals:
        problems.append(f"janela {window_id}: primeiro render real ausente")
        continue
    first_real = reals[0]
    latch_ms = (first_real["t"] - map_ns) / 1e6
    before = [p for p in placeholders if p["t"] < first_real["t"]]
    print(f"window_id={window_id} map_to_latch_ms={latch_ms:.3f} "
          f"target={first_real['target']} placeholders_antes={len(before)}")
    print("  MAP: " + map_lines[window_id].strip())
    print("  LATCH: " + first_real["raw"].strip())
    if before:
        # O primeiro placeholder pode vir com conteúdo 0×0 ou antes do shell
        # anunciar as insets; o que cobre o critério é o ÚLTIMO placeholder antes
        # do buffer real — ele precisa estar na geometria-alvo final.
        first_ph, last_ph = before[0], before[-1]
        placeholder_ms = (first_ph["t"] - map_ns) / 1e6
        aligned_ms = (last_ph["t"] - map_ns) / 1e6
        print(f"  placeholder_ms={placeholder_ms:.3f} aligned_ms={aligned_ms:.3f}")
        print("  PLACEHOLDER: " + first_ph["raw"].strip())
        print("  PLACEHOLDER_ALIGNED: " + last_ph["raw"].strip())
        # Sem configure inicial com tamanho sugerido, a janela mapeia com 0×0 e
        # o placeholder vira só as insets (12×36) — um "spike" visível na tela.
        tw, th = first_real["target"]
        if first_ph["size"][0] < tw // 2 or first_ph["size"][1] < th // 2:
            problems.append(
                f"janela {window_id}: placeholder inicial {first_ph['size']} é spike "
                f"(target {first_real['target']}); falta geometria provisória antes do commit"
            )
        if last_ph["size"] != first_real["target"]:
            problems.append(f"janela {window_id}: placeholder final {last_ph['size']} != target {first_real['target']}")
        if placeholder_ms > LIMIT_PLACEHOLDER_MS:
            problems.append(f"janela {window_id}: placeholder em {placeholder_ms:.1f} ms > {LIMIT_PLACEHOLDER_MS:.0f} ms")
    else:
        problems.append(f"janela {window_id}: nenhum placeholder antes do buffer real")
    # M12 é o PRIMEIRO OUTPUT_FRAME posterior ao latch desta janela, não o
    # primeiro do log (que acontece no startup, antes de qualquer map).
    presented = [t for t in output_frames if t >= first_real["t"]]
    if presented:
        presented_ms = (presented[0] - map_ns) / 1e6
        print(f"  presented_ms={presented_ms:.3f} latch_to_presented_ms={(presented[0] - first_real['t']) / 1e6:.3f}")
    else:
        problems.append(f"janela {window_id}: nenhum OUTPUT_FRAME após o latch")
    if window_id == 0:
        print(f"  PRIMEIRO HTML FRIO: {latch_ms:.3f} ms (sem limite: cold-start do WebProcess)")
    elif latch_ms > LIMIT_HOT_HTML_MS:
        problems.append(f"janela {window_id}: latch {latch_ms:.1f} ms > {LIMIT_HOT_HTML_MS:.0f} ms")

pool_takes = re.findall(r"FRAMEWEB: pool_take window_id=(\d+) available=(\d+)", body)
pool_ready = [int(v) for v in re.findall(r"FRAMEWEB: pool_ready available=(\d+)", body)]
anchor_alive = "FRAMEWEB: anchor viva" in body
print(f"pool_takes={pool_takes} pool_ready_sequencia={pool_ready} anchor_viva={anchor_alive}")
# eprintln do shell e do compositor compartilham o arquivo: uma linha de pool_take
# pode sair intercalada (amostra final-2). O associate só acontece DEPOIS de um
# take bem-sucedido, então é a evidência robusta de que o pool forneceu a view.
associated = sorted(set(int(v) for v in re.findall(r"associate\(window_id=(\d+)\)", body)))
print(f"associate_por_window_id={associated}")
for window_id in (0, 1, 2):
    if window_id not in associated:
        problems.append(f"pool não forneceu a janela {window_id}")
if not pool_ready or pool_ready[-1] != 2:
    problems.append(f"pool não terminou reposto com 2 views: {pool_ready}")
if not anchor_alive:
    problems.append("WebView-âncora viva não foi anunciada")

ps = subprocess.run(
    ["ps", "-eo", "pid=,ppid=,args="], check=True, text=True, stdout=subprocess.PIPE,
).stdout
processes, children = {}, {}
for line in ps.splitlines():
    fields = line.strip().split(None, 2)
    if len(fields) != 3:
        continue
    pid, ppid, args = int(fields[0]), int(fields[1]), fields[2]
    processes[pid] = (ppid, args)
    children.setdefault(ppid, []).append(pid)
shell_pids = [pid for pid, (ppid, args) in processes.items()
              if ppid == comp.pid and "frame_webview" in args]
descendants, queue = set(), list(shell_pids)
while queue:
    pid = queue.pop()
    for child in children.get(pid, []):
        if child not in descendants:
            descendants.add(child)
            queue.append(child)
web_processes = sorted(pid for pid in descendants
                       if os.path.basename(processes[pid][1].split(None, 1)[0]) == "WebKitWebProcess")
print(f"shell_pids={shell_pids} webkit_web_processes={web_processes}")
if len(shell_pids) != 1:
    problems.append(f"esperado 1 shell descendente, encontrado {shell_pids}")
if len(web_processes) != 1:
    problems.append(f"related-view deveria manter 1 WebKitWebProcess, medido {len(web_processes)}")

print(f"LOG_COMPOSITOR={LOG}")
if problems:
    print(f"FALHA: {len(problems)} problema(s)")
    for problem in problems:
        print("  " + problem)
    print("VEREDITO: FALHA")
    sys.exit(1)
print("VEREDITO: OK")
