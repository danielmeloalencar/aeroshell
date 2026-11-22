#!/usr/bin/env python3
"""02r: maximizar ocupa exatamente a area nao exclusiva do output.

Mede tres coisas, sem instrumentacao nova de producao:
  (a) a area util vem do proprio LayerMap do compositor (log
      `geometria real da layer surface (LayerMap)` -> zone=Rectangle {...});
  (b) o alvo do frame vem do log `frame configure (conteudo do cliente)`
      (total_w/total_h = conteudo real do cliente + insets/sombra do frame);
  (c) a posicao do cliente vem do IPC `list` (`W id x y w h <titulo>`).

O painel do AeroShell e uma layer surface ancorada na BORDA INFERIOR com
exclusive_zone=48 (host.rs:7-19); por isso a zona util e (0,0,output_w,output_h-48)
e o vazamento se ve na BORDA INFERIOR do frame, nao na superior.

O foot arredonda o tamanho pedido para a grade de celulas (frame_resize_sync.py:166:
want=706 -> conteudo real=702), entao as comparacoes de tamanho usam tolerancia.
A origem, porem, e exata (inteiros do Space), tolerancia de 2 px.

Uma execucao:
  cargo build --workspace --examples
  python3 tests/integration/maximize_workarea.py
"""
import atexit
import os
import re
import socket
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMP = os.path.join(ROOT, "target/debug/aeroshell-compositor")
FRAME = os.path.join(ROOT, "target/debug/examples/frame_webview")
PANEL = os.path.join(ROOT, "target/debug/aeroshell-shell")

OUTDIR = "/tmp/opencode/02r"
os.makedirs(OUTDIR, exist_ok=True)
SOCK = f"wayland-aero-maxwork-{os.getpid()}"
LOG = os.path.join(OUTDIR, f"comp-{os.getpid()}.log")
IPC_SOCK = os.path.join(OUTDIR, f"ipc-{os.getpid()}.sock")

OUTPUT_SIZE = (800, 600)
# Tolerancia da grade do foot (largura) e da altura de celula (altura).
TOL_W, TOL_H = 16, 20
TOL_POS = 2

for orphan in (f"/run/user/{os.getuid()}/{SOCK}", IPC_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)

env = os.environ.copy()
for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
    env.pop(key, None)
env.update(RUST_LOG="debug", AERO_WINDOW_IPC_SOCK=IPC_SOCK)

log_handle = open(LOG, "w")
comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK,
     "--size", f"{OUTPUT_SIZE[0]}x{OUTPUT_SIZE[1]}",
     "--command", "foot", "--shell", FRAME,
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
    env=env, stdout=log_handle, stderr=subprocess.STDOUT,
)

panel_env = os.environ.copy()
panel_env.update(
    WAYLAND_DISPLAY=SOCK,
    GDK_BACKEND="wayland",
    GTK4_LAYER_SHELL_LIB=os.environ.get("GTK4_LAYER_SHELL_LIB", "libgtk4-layer-shell.so.0"),
    RUST_LOG="info",
)
panel = subprocess.Popen(
    [PANEL], env=panel_env, stdout=log_handle, stderr=subprocess.STDOUT,
)


def stop(proc):
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()


def cleanup():
    stop(panel)
    stop(comp)
    log_handle.close()


atexit.register(cleanup)


def log():
    return re.sub(r"\x1b\[[0-9;]*m", "", open(LOG, errors="ignore").read())


def last(pattern, group=0):
    found = re.findall(pattern, log())
    return found[-1] if found else None


def near(a, b, tol):
    return a is not None and b is not None and abs(a - b) <= tol


def wait_for(predicate, seconds=30):
    deadline = time.time() + seconds
    while time.time() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.2)
    return None


# --- area util (LayerMap) -------------------------------------------------
ZONE_RE = (
    r"geometria real da layer surface \(LayerMap\).*?"
    r"zone=Rectangle<[^>]+> \{ x: (-?\d+), y: (-?\d+), width: (\d+), height: (\d+) \}"
)


def zone():
    m = last(ZONE_RE)
    return None if m is None else tuple(int(v) for v in m)


# --- insets e sombra declarados pelo shell --------------------------------
def insets():
    m = last(r"insets window_id=0 left=(\d+) top=(\d+) right=(\d+) bottom=(\d+)")
    return tuple(int(v) for v in m) if m else (0, 0, 0, 0)


def shadow():
    m = last(r"sombra window_id=0 left=(\d+) top=(\d+) right=(\d+) bottom=(\d+)")
    return tuple(int(v) for v in m) if m else (0, 0, 0, 0)


# --- configuracao do frame ------------------------------------------------
CFG_RE = (
    r"frame configure \(conteúdo do cliente\).*?"
    r"window_id=(\d+).*?content_w=(\d+).*?content_h=(\d+).*?"
    r"total_w=(\d+).*?total_h=(\d+)"
)


def frame_config():
    m = last(CFG_RE)
    return None if m is None else tuple(int(v) for v in m[1:])


# --- IPC ------------------------------------------------------------------
def ipc(command):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(4)
    s.connect(IPC_SOCK)
    s.sendall((command + "\n").encode())
    data = b""
    while b"\n" not in data:
        chunk = s.recv(4096)
        if not chunk:
            break
        data += chunk
    s.close()
    return data.decode(errors="ignore").strip()


def client_geo():
    for line in ipc("list").splitlines():
        parts = line.split()
        if len(parts) >= 6 and parts[0] == "W" and parts[1] == "0":
            return tuple(int(v) for v in parts[2:6])
    return None


def wait_config_change(previous_cfg, seconds=25, want_near=None, tol=(1, 1, 1, 1)):
    """Espera um configure cuja tupla (conteúdo+alvo) mudou, ou voltou ao
    esperado. A comparação é da tupla inteira: na startup o configure das insets
    muda só o total (mesmo conteúdo), por isso o baseline é estabilizado antes
    (configured_with_insets) e aqui qualquer diferença já é o maximize."""
    def ready():
        cfg = frame_config()
        if cfg is None:
            return None
        if want_near is not None:
            return cfg if all(abs(a - b) <= t for a, b, t in zip(cfg, want_near, tol)) else None
        return cfg if any(abs(a - b) > t for a, b, t in zip(cfg, previous_cfg, tol)) else None
    return wait_for(ready, seconds)


# --- espera o estado estavel ----------------------------------------------
if not wait_for(lambda: os.path.exists(IPC_SOCK), 20):
    print("FALHA: socket IPC nao apareceu", file=sys.stderr)
    sys.exit(1)
if not wait_for(lambda: zone() and zone()[3] < OUTPUT_SIZE[1], 40):
    print("FALHA: painel nao reduziu a area util (zone)", file=sys.stderr)
    print(log()[-3000:], file=sys.stderr)
    sys.exit(1)
if not wait_for(client_geo, 25):
    print("FALHA: janela 0 nao apareceu no IPC", file=sys.stderr)
    sys.exit(1)
if not wait_for(frame_config, 45):
    print("FALHA: configure do frame nao chegou", file=sys.stderr)
    print(log()[-3000:], file=sys.stderr)
    sys.exit(1)

z = zone()
i = insets()
sh = shadow()
extra_w = i[0] + i[2] + sh[0] + sh[2]
extra_h = i[1] + i[3] + sh[1] + sh[3]
off_x, off_y = -(i[0] + sh[0]), -(i[1] + sh[1])

# Estabiliza: espera o configure que JÁ inclui as insets declaradas. Sem isto
# o baseline seria o configure 798x598 sem decoração e a leitura seguinte
# pegaria o configure das insets como se fosse o do maximize.
def configured_with_insets():
    cfg = frame_config()
    return cfg if cfg and sum(i) > 0 and (cfg[2], cfg[3]) == (cfg[0] + extra_w, cfg[1] + extra_h) else None

if not wait_for(configured_with_insets, 25):
    print("FALHA: configure do frame com insets nao chegou", file=sys.stderr)
    print(log()[-3000:], file=sys.stderr)
    sys.exit(1)

before_geo = client_geo()
before_cfg = frame_config()
print(f"zona_util (x,y,w,h) = {z}")
print(f"insets={i} sombra={sh} extra=({extra_w},{extra_h}) offset=({off_x},{off_y})")
print(f"ANTES  cliente (x,y,w,h)={before_geo} frame alvo (w,h)={before_cfg[2:]} "
      f"conteudo={before_cfg[:2]} frame_origem="
      f"({before_geo[0] + off_x},{before_geo[1] + off_y})")

want_client_loc = (z[0] - off_x, z[1] - off_y)
want_client_size = (z[2] - extra_w, z[3] - extra_h)

# --- maximizar ------------------------------------------------------------
reply_max = ipc("maximize 0")
if not wait_config_change(before_cfg):
    print("FALHA: frame nao reconfigurou apos maximize", file=sys.stderr)
    sys.exit(1)
after_geo = client_geo()
after_cfg = frame_config()
frame_origin = (after_geo[0] + off_x, after_geo[1] + off_y)
print(f"DEPOIS cliente (x,y,w,h)={after_geo} frame alvo (w,h)={after_cfg[2:]} "
      f"conteudo={after_cfg[:2]} frame_origem={frame_origin}")
print(f"ESPERADO cliente loc={want_client_loc} size~{want_client_size} "
      f"frame alvo~({z[2]},{z[3]}) frame_origem={z[:2]}  [resposta IPC: {reply_max}]")

# --- desmaximizar ---------------------------------------------------------
reply_restore = ipc("maximize 0")
if not wait_config_change(after_cfg, want_near=before_cfg, tol=(TOL_W, TOL_H, TOL_W, TOL_H)):
    print("FALHA: frame nao reconfigurou apos desmaximizar", file=sys.stderr)
    sys.exit(1)
restored_geo = client_geo()
restored_cfg = frame_config()
print(f"RESTAURADO cliente (x,y,w,h)={restored_geo} frame alvo (w,h)={restored_cfg[2:]} "
      f"conteudo={restored_cfg[:2]}  [resposta IPC: {reply_restore}]")

# --- excecoes de protocolo ------------------------------------------------
protocol_errors = [
    line for line in log().splitlines()
    if re.search(r"ProtocolError|protocol error|invalid_size|\[.*ERROR.*\]", line)
]
print(f"erros_de_protocolo={len(protocol_errors)}")
for line in protocol_errors[:3]:
    print(f"  {line}")

# --- veredito -------------------------------------------------------------
checks = {
    "maximize aceito": reply_max == "OK" and reply_restore == "OK",
    "origem do frame = origem da zona": (
        near(frame_origin[0], z[0], TOL_POS) and near(frame_origin[1], z[1], TOL_POS)
    ),
    "alvo do frame = tamanho da zona": (
        near(after_cfg[2], z[2], TOL_W) and near(after_cfg[3], z[3], TOL_H)
    ),
    "sem vazamento inferior": after_cfg[3] <= z[3] + TOL_H,
    "cliente = zona - insets": (
        near(after_cfg[0], want_client_size[0], TOL_W)
        and near(after_cfg[1], want_client_size[1], TOL_H)
    ),
    "nao usa tamanho bruto do output": (
        after_cfg[0] < OUTPUT_SIZE[0] and after_cfg[1] < OUTPUT_SIZE[1]
    ),
    "restaura posicao": (
        near(restored_geo[0], before_geo[0], TOL_POS)
        and near(restored_geo[1], before_geo[1], TOL_POS)
    ),
    "restaura tamanho": (
        near(restored_cfg[0], before_cfg[0], TOL_W)
        and near(restored_cfg[1], before_cfg[1], TOL_H)
    ),
    "sem excecao de protocolo": not protocol_errors,
}
print()
for name, ok in checks.items():
    print(f"  [{'OK' if ok else 'XX'}] {name}")
verdict = all(checks.values())
print(f"\nVEREDITO: {'OK' if verdict else 'FALHA'}")
sys.exit(0 if verdict else 1)
