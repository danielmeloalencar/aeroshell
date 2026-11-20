#!/usr/bin/env python3
"""02e: medir a colagem ESPACIAL da decoração ao conteúdo commitado.

O compositor compilado com `--features bench` registra, por render com frame
associado:
  RESIZE_GLUE t=<ns> window_id=<id> content=<w>x<h> target=<w>x<h>
              buffer=<w>x<h> rescaled=<bool>

Invariante: target == conteúdo COMMITADO + extras (insets + shadow) e
Na arquitetura 02p, todo frame associado usa 9-slice (rescaled=true) e o canvas
permanece constante no grab/release; não existe mais swap para buffer nativo.

VM sem 3D: nenhuma conclusão de desempenho; apenas geometria. Nenhuma amostra
é descartada em silêncio — as linhas cruas são impressas.
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

SOCK = f"wayland-aero-glue-{os.getpid()}"
LOG = f"/tmp/glue-comp-{os.getpid()}.log"
INPUT_SOCK = f"/tmp/glue-input-{os.getpid()}.sock"
STEPS = 14
STEP_PX = 10
MOTION_DELAY = 0.05
SETTLE_MS = 500
TOL = 1
# 02k: a espessura renderizada das fatias fixas tem de bater com os insets.
# 0,25 px aceita arredondamento de raster, mas detecta a variação medida no
# baseline (edge_l de 6,05 a 6,36 px com insets=6).
EDGE_TOL = 0.25
# 02k: passo do gesto é 10 px sobre ~500 px de altura => d(escala) ~= 0,02.
# 0,05 aceita a variação normal do arrasto e detecta um snap de 0,1 ou mais.
SNAP_LIMIT = 0.05

for orphan in (f"/run/user/1000/{SOCK}", INPUT_SOCK):
    if os.path.exists(orphan):
        os.unlink(orphan)
time.sleep(1)

# A instrumentação só existe com a feature bench (AGENTS.md: variáveis/knobs de bench).
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
env.update(RUST_LOG="debug", AERO_TEST_INPUT_SOCK=INPUT_SOCK)

comp = subprocess.Popen(
    [COMP, "--backend", "winit", "--socket", SOCK,
     "--command", "foot", "--shell", FRAME,
     "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
    env=env, stdout=open(LOG, "w"), stderr=subprocess.STDOUT,
)


def cleanup():
    """Encerra apenas o PID iniciado aqui."""
    if comp.poll() is None:
        comp.terminate()
        try:
            comp.wait(timeout=5)
        except subprocess.TimeoutExpired:
            comp.kill()


atexit.register(cleanup)


def log():
    return re.sub(r"\x1b\[[0-9;]*m", "", open(LOG, errors="ignore").read())


for _ in range(200):
    if os.path.exists(INPUT_SOCK):
        break
    time.sleep(0.05)
else:
    print("FALHA: socket de input sintético não apareceu", file=sys.stderr)
    sys.exit(1)

config = None
for _ in range(150):
    m = re.findall(
        r"frame configure \(conteúdo do cliente\).*?content_w=(\d+).*?content_h=(\d+)"
        r".*?total_w=(\d+).*?total_h=(\d+)",
        log(),
    )
    if m:
        config = tuple(int(v) for v in m[-1])
        break
    time.sleep(0.1)
if not config:
    print("FALHA: configure do frame não chegou", file=sys.stderr)
    print(log()[-2000:], file=sys.stderr)
    sys.exit(1)

# O WebView pode levar alguns segundos para produzir o PRIMEIRO buffer. Sem ele
# não há elemento para importar no render e a instrumentação não emite linhas.
# Esperar o pré-requisito evita falsa falha por infraestrutura (amostra crua de
# /tmp/glue-comp-58443.log: 28 FRAME_BUFFER_COMMIT, todos com buffer=None).
for _ in range(200):
    if re.search(r"FRAME_BUFFER_COMMIT buffer=Some\(\(\d+, \d+\)\)", log()):
        break
    time.sleep(0.1)
else:
    print("FALHA: frame WebView não commitou buffer inicial", file=sys.stderr)
    print(log()[-2000:], file=sys.stderr)
    sys.exit(1)

extras = (config[2] - config[0], config[3] - config[1])
INSETS = (6, 30, 6, 6)
content_origin = (60, 80)
frame_origin = (content_origin[0] - INSETS[0], content_origin[1] - INSETS[1])
total = (config[0] + extras[0], config[1] + extras[1])
# 02k: arrastar o CANTO inferior direito para medir borda (X) e titlebar (Y)
# no mesmo gesto. O baseline 02e só variava X, deixando title_h sem medição.
edge_x = frame_origin[0] + total[0] - 3
edge_y = frame_origin[1] + total[1] - 3
print(f"conteudo_inicial={config[:2]} extras={extras} total={total} canto=({edge_x},{edge_y})")
print(f"parametros: passos={STEPS} delta={STEP_PX}px delay={MOTION_DELAY:.3f}s settle={SETTLE_MS}ms")

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(INPUT_SOCK)


def send(line, delay=MOTION_DELAY):
    s.sendall((line + "\n").encode())
    time.sleep(delay)


send(f"motion {edge_x} {edge_y}")
send("button press", 0.1)
for i in range(STEPS):
    send(f"motion {edge_x + (i + 1) * STEP_PX} {edge_y + (i + 1) * STEP_PX}")
send("button release", 0.1)
release_ns = time.time_ns()
time.sleep(1.2)

text = log()
lines = [l for l in text.splitlines() if "RESIZE_GLUE " in l]
glue = []
malformed = []
for l in lines:
    m = re.search(
        r"RESIZE_GLUE t=(\d+) window_id=(\d+) content=(\d+)x(\d+) "
        r"target=(\d+)x(\d+) buffer=(\d+)x(\d+) rescaled=(true|false)", l,
    )
    if not m:
        # Interleaving de stdout entre compositor e filhos pode corromper uma
        # linha. Contabilizamos em vez de descartar em silêncio.
        malformed.append(l)
        continue
    if m:
        t, wid = int(m.group(1)), int(m.group(2))
        content = (int(m.group(3)), int(m.group(4)))
        target = (int(m.group(5)), int(m.group(6)))
        buffer = (int(m.group(7)), int(m.group(8)))
        glue.append((t, wid, content, target, buffer, m.group(9) == "true", l))

print(f"\nlinhas RESIZE_GLUE={len(glue)} malformadas={len(malformed)}")
if malformed:
    print("exemplos malformados: " + repr(malformed[:3]))
print("\n--- trecho cru das medições (primeiras e últimas 15 linhas) ---")
print("\n".join(lines[:15]))
print("...")
print("\n".join(lines[-15:]))

if not glue:
    print("FALHA: nenhuma linha RESIZE_GLUE (instrumentação ausente?)", file=sys.stderr)
    sys.exit(1)

problems = []
during = [g for g in glue if g[0] < release_ns]
if not during:
    problems.append("nenhuma linha RESIZE_GLUE durante o grab")

for t, wid, content, target, buffer, rescaled, raw in during:
    if min(content) <= 0 or min(buffer) <= 0:
        continue
    want = (content[0] + extras[0], content[1] + extras[1])
    if any(abs(a - b) > TOL for a, b in zip(target, want)):
        problems.append(f"target {target} != content+extras {want}")
    if not rescaled:
        problems.append(f"frame abandonou 9-slice: buffer={buffer} target={target}")

settled = [g for g in glue if release_ns <= g[0] <= release_ns + SETTLE_MS * 1_000_000]
ok_settle = [g for g in settled if g[5] and min(g[2]) > 0]
if not settled:
    problems.append(f"nenhuma linha RESIZE_GLUE nos {SETTLE_MS} ms após soltar")
elif not ok_settle:
    problems.append("após soltar: frame fora do 9-slice")
elif len({g[4] for g in settled}) != 1:
    problems.append(f"canvas mudou no release: {sorted({g[4] for g in settled})}")

print(f"durante_grab={len(during)} apos_soltar={len(settled)} colados={len(ok_settle)}")

# --- 02k: espessura das fatias fixas e ausência de snap -------------------
# O compositor (bench) emite uma linha por frame renderizado com o frame
# associado. Nenhuma amostra é descartada em silêncio: linhas malformadas
# são contabilizadas e impressas.
SLICE_RE = re.compile(
    r"RESIZE_SLICE t=(\d+) window_id=(\d+) buf=(\d+)x(\d+) "
    r"target=(\d+)x(\d+) rescaled=(true|false) "
    r"insets=(\d+),(\d+),(\d+),(\d+) "
    r"edge_l=([0-9.]+) edge_r=([0-9.]+) title_h=([0-9.]+) "
    r"slice_scale_x=([0-9.]+) slice_scale_y=([0-9.]+) "
    r"title_n=(\d+) title_slices=(\d+),(\d+) "
    r"title_left_w=([0-9.]+) title_gap_w=([0-9.]+) title_right_w=([0-9.]+)"
)

slices = []
malformed_slices = []
for l in text.splitlines():
    if "RESIZE_SLICE " not in l:
        continue
    m = SLICE_RE.search(l)
    if not m:
        malformed_slices.append(l)
        continue
    g = m.groups()
    slices.append({
        "t": int(g[0]), "wid": int(g[1]),
        "buf": (int(g[2]), int(g[3])), "target": (int(g[4]), int(g[5])),
        "rescaled": g[6] == "true",
        "insets": tuple(int(x) for x in g[7:11]),
        "edge_l": float(g[11]), "edge_r": float(g[12]), "title_h": float(g[13]),
        "sx": float(g[14]), "sy": float(g[15]),
        "title_n": int(g[16]),
        "title_slices": (int(g[17]), int(g[18])),
        "title_left_w": float(g[19]),
        "title_gap_w": float(g[20]),
        "title_right_w": float(g[21]),
        "raw": l,
    })

slice_during = [s for s in slices if s["t"] < release_ns and s["rescaled"]]
print(f"\nlinhas RESIZE_SLICE={len(slices)} malformadas={len(malformed_slices)} "
      f"durante_grab={len(slice_during)}")
if malformed_slices:
    print("exemplos SLICE malformados: " + repr(malformed_slices[:3]))
if slices:
    print("--- trecho cru das fatias (primeiras e últimas 8) ---")
    print("\n".join(s["raw"] for s in slices[:8]))
    print("...")
    print("\n".join(s["raw"] for s in slices[-8:]))

if not slices:
    problems.append("nenhuma linha RESIZE_SLICE (instrumentação 02k ausente?)")
elif not slice_during:
    problems.append("nenhuma linha RESIZE_SLICE com rescaled=true durante o grab")
else:
    def faixa(campo):
        vals = [s[campo] for s in slice_during]
        return f"[{min(vals):.2f},{max(vals):.2f}]"

    print(f"faixas: edge_l={faixa('edge_l')} edge_r={faixa('edge_r')} "
          f"title_h={faixa('title_h')} insets={slice_during[0]['insets']} "
          f"(tolerancia={EDGE_TOL})")
    edge_bad = []
    for s in slice_during:
        want_l, want_t, want_r = s["insets"][0], s["insets"][1], s["insets"][2]
        if abs(s["edge_l"] - want_l) > EDGE_TOL:
            edge_bad.append(f"t={s['t']} edge_l={s['edge_l']:.2f}!={want_l}")
        if abs(s["edge_r"] - want_r) > EDGE_TOL:
            edge_bad.append(f"t={s['t']} edge_r={s['edge_r']:.2f}!={want_r}")
        if abs(s["title_h"] - want_t) > EDGE_TOL:
            edge_bad.append(f"t={s['t']} title_h={s['title_h']:.2f}!={want_t}")
    if edge_bad:
        problems.append(
            f"espessura variou ({len(edge_bad)} amostras; "
            f"ex.: {', '.join(edge_bad[:3])})"
        )

    # --- 02l: texto e botões da titlebar não podem ser esticados ------------
    # O compositor informa quantas fatias compõem a barra, as larguras
    # declaradas pelo shell (texto, botões) e as larguras DESTINO de cada
    # zona. Baseline 02k: title_n=1 e a zona esquerda cresce com slice_sx.
    declared = {s["title_slices"] for s in slice_during}
    left_bad = [
        f"t={s['t']} title_left_w={s['title_left_w']:.2f}!={s['title_slices'][0]}"
        for s in slice_during
        if abs(s["title_left_w"] - s["title_slices"][0]) > EDGE_TOL
    ]
    right_bad = [
        f"t={s['t']} title_right_w={s['title_right_w']:.2f}!={s['title_slices'][1]}"
        for s in slice_during
        if abs(s["title_right_w"] - s["title_slices"][1]) > EDGE_TOL
    ]
    total_bad = []
    for s in slice_during:
        want = s["target"][0] - s["insets"][0] - s["insets"][2]
        got = s["title_left_w"] + s["title_gap_w"] + s["title_right_w"]
        if abs(got - want) > 1:
            total_bad.append(f"t={s['t']} soma={got:.2f}!={want}")
    gaps = [s["title_gap_w"] for s in slice_during]
    print(f"titlebar: n={ {s['title_n'] for s in slice_during} } "
          f"declaradas={declared} "
          f"left=[{min(s['title_left_w'] for s in slice_during):.2f},"
          f"{max(s['title_left_w'] for s in slice_during):.2f}] "
          f"right=[{min(s['title_right_w'] for s in slice_during):.2f},"
          f"{max(s['title_right_w'] for s in slice_during):.2f}] "
          f"gap=[{min(gaps):.2f},{max(gaps):.2f}]")
    if {s["title_n"] for s in slice_during} != {3}:
        problems.append(
            f"titlebar composta por { {s['title_n'] for s in slice_during} } "
            f"fatias (esperado 3: texto, intervalo, botões)"
        )
    if len(declared) != 1 or min(declared) == (0, 0):
        problems.append(f"linhas de corte da titlebar ausentes/inconsistentes: {declared}")
    if left_bad:
        problems.append(
            f"zona do texto esticada ({len(left_bad)} amostras; ex.: {left_bad[:2]})"
        )
    if right_bad:
        problems.append(
            f"zona dos botões esticada ({len(right_bad)} amostras; ex.: {right_bad[:2]})"
        )
    if total_bad:
        problems.append(
            f"soma das zonas != largura alvo ({len(total_bad)} amostras; "
            f"ex.: {total_bad[:2]})"
        )
    if max(gaps) - min(gaps) < 1:
        problems.append("o intervalo esticável não acompanhou o resize")

    # Snap = salto descontínuo da escala das fatias entre frames consecutivos.
    jumps = []
    for a, b in zip(slice_during, slice_during[1:]):
        jumps.append((abs(b["sx"] - a["sx"]), "sx", a["t"], b["t"]))
        jumps.append((abs(b["sy"] - a["sy"]), "sy", a["t"], b["t"]))
    max_jump = max(jumps) if jumps else (0.0, "-", 0, 0)
    print(f"salto_maximo_escala={max_jump[0]:.4f} em {max_jump[1]} "
          f"(limite={SNAP_LIMIT})")
    if max_jump[0] > SNAP_LIMIT:
        problems.append(
            f"snap da escala das fatias: delta={max_jump[0]:.4f} "
            f"em {max_jump[1]} t={max_jump[2]}->{max_jump[3]}"
        )

# Prova de que o caminho do 9-slice foi executado (não é só a medição se
# reportando): cada frame escalado durante o grab deve compor exatamente 9
# fatias. Uma fatia com crop vazio reduziria n e é falha.
compose = []
malformed_compose = []
for l in text.splitlines():
    if "SLICE_COMPOSE " not in l:
        continue
    m = re.search(r"SLICE_COMPOSE t=(\d+) window_id=(\d+) n=(\d+)", l)
    if m:
        compose.append((int(m.group(1)), int(m.group(3))))
    else:
        malformed_compose.append(l)
compose_during = [c for c in compose if c[0] < release_ns]
print(f"linhas SLICE_COMPOSE={len(compose)} durante_grab={len(compose_during)} "
      f"malformadas={len(malformed_compose)} n_min="
      f"{min((n for _, n in compose_during), default='-')}")
if not compose_during:
    problems.append("nenhuma linha SLICE_COMPOSE durante o grab")
elif any(n != 9 for _, n in compose_during):
    bad = [(t, n) for t, n in compose_during if n != 9][:3]
    problems.append(f"composição com != 9 fatias: {bad}")

# Geometria REAL dos elementos compostos (não reusa os valores que o próprio
# compositor calculou para RESIZE_SLICE). As 3 primeiras fatias são o topo.
GEOM_RE = re.compile(
    r"SLICE_GEOM t=(\d+) window_id=(\d+) i=(\d+) "
    r"x=(-?\d+) y=(-?\d+) w=(\d+) h=(\d+)"
)
geoms = {}
malformed_geom = []
for l in text.splitlines():
    if "SLICE_GEOM " not in l:
        continue
    m = GEOM_RE.search(l)
    if not m:
        malformed_geom.append(l)
        continue
    t, _wid, idx, x, y, w, h = (int(v) for v in m.groups())
    geoms.setdefault(t, {})[idx] = (x, y, w, h)
slice_by_t = {s["t"]: s for s in slice_during}
geom_during = {t: g for t, g in geoms.items() if t < release_ns and t in slice_by_t}
print(f"linhas SLICE_GEOM={sum(len(g) for g in geoms.values())} malformadas="
      f"{len(malformed_geom)} frames_durante_grab={len(geom_during)}")
if not geom_during:
    problems.append("nenhuma linha SLICE_GEOM durante o grab")
else:
    geom_bad = []
    for t, g in sorted(geom_during.items()):
        if len(g) != 9:
            geom_bad.append(f"t={t} n_geoms={len(g)}")
            continue
        s = slice_by_t[t]
        # As fatias de canto incluem o inset lateral (6 px), declarado à parte.
        want_l = s["insets"][0] + s["title_slices"][0]
        want_r = s["insets"][2] + s["title_slices"][1]
        w0, w1, w2 = g[0][2], g[1][2], g[2][2]
        if abs(w0 - want_l) > 1:
            geom_bad.append(f"t={t} texto w={w0}!={want_l}")
        if abs(w2 - want_r) > 1:
            geom_bad.append(f"t={t} botoes w={w2}!={want_r}")
        if abs(w1 - s["title_gap_w"]) > 1:
            geom_bad.append(f"t={t} intervalo w={w1}!={s['title_gap_w']:.0f}")
        if g[1][0] != g[0][0] + w0 or g[2][0] != g[1][0] + w1:
            geom_bad.append(f"t={t} topo nao contiguo: {g[0][0]},{g[1][0]},{g[2][0]}")
        if len({g[i][1] for i in range(3)}) != 1 or any(g[i][3] != s["insets"][1] for i in range(3)):
            geom_bad.append(f"t={t} altura do topo inconsistente")
        # As 9 fatias têm de cobrir exatamente o retângulo alvo. Sem o
        # deslocamento das bordas direita/inferior, elas ficam nas coordenadas
        # do buffer congelado (1292x836) e o canto some (VM: output_scale=1).
        xmax = max(g[i][0] + g[i][2] for i in range(9))
        ymax = max(g[i][1] + g[i][3] for i in range(9))
        if xmax != g[0][0] + s["target"][0]:
            geom_bad.append(f"t={t} borda direita em x={xmax}, alvo={g[0][0] + s['target'][0]}")
        if ymax != g[0][1] + s["target"][1]:
            geom_bad.append(f"t={t} borda inferior em y={ymax}, alvo={g[0][1] + s['target'][1]}")
    if geom_bad:
        problems.append(
            f"geometria real das fatias errada ({len(geom_bad)}; "
            f"ex.: {', '.join(geom_bad[:3])})"
        )

if problems:
    print("FALHA: " + "; ".join(problems[:5]), file=sys.stderr)
    verdict = "FALHA"
else:
    verdict = "OK"
print(f"\nVEREDITO: {verdict}")
print("(esperado: target == content + extras; 9-slice em todo frame; canvas constante; "
      "edge_l/edge_r/title_h == insets e "
      f"sem salto de escala > {SNAP_LIMIT} durante o grab)")
sys.exit(0 if verdict == "OK" else 1)
