#!/usr/bin/env python3
"""F2-shell-real: medidor objetivo de redesenho/geometry (ANTES de qualquer fix).

90 s de nested winit com painel + 2 foot, sem nenhuma interação nem edição de
arquivo. Conta:
  - commits de cliente (`FOOT_COMMIT`) e `frame_painted` por segundo;
  - configurações de frame/layer surface e tamanhos distintos (tremor);
  - mensagens `panel-state`/`interactive` emitidas pelo React (re-layout);
  - `POLICY_TS RELOAD`/DISPATCH sem o .ts ter mudado (falso positivo do fs.watch).

Referência: idle < 5 commits/s; vsync travado ≈ 60/s. A comparação A/B usa
AERO_ROOT apontando para checkouts diferentes (atual e commit estável).

Uso:
  AERO_ROOT=<repo> PROBE_SECS=90 LABEL=atual python3 tests/integration/f2_shell_real_redraw_probe.py
"""
import collections
import json
import os
import pathlib
import re
import signal
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(os.environ.get("AERO_ROOT", pathlib.Path(__file__).resolve().parents[2]))
RUNTIME_DIR = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
SECS = int(os.environ.get("PROBE_SECS", "90"))
LABEL = os.environ.get("LABEL", "atual")
OUT = pathlib.Path(tempfile.mkdtemp(prefix=f"aero-redraw-{LABEL}-"))
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def clean(path):
    return ANSI.sub("", path.read_text(errors="ignore")) if path.exists() else ""


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


def rate_per_sec(values, elapsed):
    return round(len(values) / max(elapsed, 1.0), 2)


def main():
    config_home = OUT / "config"
    (config_home / "aeroshell").mkdir(parents=True, exist_ok=True)
    home = OUT / "home"
    home.mkdir(parents=True, exist_ok=True)
    rules = config_home / "aeroshell" / "rules.ts"
    rules.write_text(
        'import type { DecorationPolicyConfig } from "'
        + str(ROOT / "packages/sdk/src/generated/DecorationPolicyConfig")
        + '";\n'
        'const config: DecorationPolicyConfig = { default: "Server", rules: [\n'
        '  { app_id: null, class: "gedit", policy: "Client" },\n'
        '  { app_id: "foot", class: null, policy: "Server" },\n'
        '] };\nexport default config;\n'
    )

    sock = f"wayland-aero-redraw-{LABEL}-{os.getpid()}"
    ipc = OUT / "ipc.sock"
    comp_log = OUT / "comp.log"
    shell_log = OUT / "shell.log"
    runtime_log = OUT / "runtime.log"
    env = {**os.environ, "RUST_LOG": "info", "AERO_WINDOW_IPC_SOCK": str(ipc),
           "XDG_CONFIG_HOME": str(config_home), "HOME": str(home)}
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD", "AERO_DECORATION_RULES"):
        env.pop(key, None)

    comp = shell = runtime = None
    feet = []
    try:
        handles = {p: p.open("wb") for p in (comp_log, shell_log, runtime_log)}
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "winit",
             "--socket", sock, "--size", "1000x600",
             "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, env=env, stdout=handles[comp_log], stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        ready = wait_for(lambda: ipc.exists() and (RUNTIME_DIR / sock).exists(), 15)
        if not ready:
            print("FALHA: compositor não subiu", file=sys.stderr)
            return 1

        runtime = subprocess.Popen(
            ["bun", str(ROOT / "packages/runtime/src/policy.ts")],
            cwd=ROOT, env={**env, "AERO_WINDOW_IPC_SOCK": str(ipc)},
            stdout=handles[runtime_log], stderr=subprocess.STDOUT, start_new_session=True,
        )
        wait_for(lambda: "POLICY_TS READY" in clean(runtime_log), 15)
        shell = subprocess.Popen(
            ["dbus-run-session", "--", str(ROOT / "target/debug/aeroshell-shell")],
            cwd=ROOT,
            env={**env, "WAYLAND_DISPLAY": sock, "GDK_BACKEND": "wayland",
                 "GTK4_LAYER_SHELL_LIB": os.environ.get("GTK4_LAYER_SHELL_LIB", "libgtk4-layer-shell.so.0")},
            stdout=handles[shell_log], stderr=subprocess.STDOUT, start_new_session=True,
        )
        time.sleep(3)
        for _ in range(2):
            feet.append(subprocess.Popen(
                ["foot"], cwd=ROOT, env={**env, "WAYLAND_DISPLAY": sock},
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True,
            ))
            time.sleep(1)
        if not wait_for(lambda: "POLICY_TS DECISION" in clean(runtime_log), 15):
            print("FALHA: nenhuma decisão de política", file=sys.stderr)
            return 1

        # --- janela de amostragem: NADA é editado nem clicado ----------------
        t0 = time.time()
        time.sleep(SECS)
        elapsed = time.time() - t0
        comp_text, shell_text, runtime_text = clean(comp_log), clean(shell_log), clean(runtime_log)

        commits = [int(ns) for ns in re.findall(r"FOOT_COMMIT t=(\d+)", comp_text)]
        painted = [int(ns) for ns in re.findall(r"EVENT_PUSH t=(\d+) kind=frame_painted", comp_text)]
        painted_dom = re.findall(r"FRAMEWEB: frame_painted window_id=(\d+)", comp_text)
        buffer_commits = re.findall(r"FRAME_BUFFER_COMMIT buffer=", comp_text)
        panel_state = re.findall(r"panel-state", shell_text)
        interactive = re.findall(r"input region aplicada", shell_text)
        reloads = re.findall(r"POLICY_TS RELOAD source=", runtime_text)
        reload_dispatch = re.findall(r"POLICY_TS RELOAD_DISPATCH", runtime_text)
        reload_errors = re.findall(r"POLICY_TS RELOAD ERROR", runtime_text)

        configures = re.findall(
            r"frame configure \(conteúdo do cliente\) window_id=(\d+).*?"
            r"content_w=(\d+) content_h=(\d+) total_w=(\d+) total_h=(\d+) "
            r"canvas_w=(\d+) canvas_h=(\d+)",
            comp_text,
        )
        sizes = collections.defaultdict(set)
        for wid, cw, ch, tw, th, an, ah in configures:
            sizes[wid].add((int(cw), int(ch), int(tw), int(th)))
        layer_sizes = set(re.findall(r"tamanho aceito pela layer surface size=([^\n]+)", comp_text))
        layer_geom = set(re.findall(
            r"geometria real da layer surface \(LayerMap\).*?geometry=Some\(Rectangle[^)]*\)\s*"
            r"\{\s*x:\s*(-?\d+),\s*y:\s*(-?\d+)", comp_text, re.S,
        ))

        metrics = {
            "label": LABEL, "root": str(ROOT), "elapsed_s": round(elapsed, 1),
            "commit_rate_per_s": rate_per_sec(commits, elapsed),
            "frame_painted_rate_per_s": rate_per_sec(painted, elapsed),
            "frame_painted_dom_rate_per_s": rate_per_sec(painted_dom, elapsed),
            "frame_buffer_commit_rate_per_s": rate_per_sec(buffer_commits, elapsed),
            "panel_state_rate_per_s": rate_per_sec(panel_state, elapsed),
            "interactive_rate_per_s": rate_per_sec(interactive, elapsed),
            "frame_configures_total": len(configures),
            "distinct_frame_sizes": {k: len(v) for k, v in sizes.items()},
            "layer_sizes_distinct": len(layer_sizes),
            "layer_geometries_distinct": len(layer_geom),
            "policy_reloads_without_edit": len(reloads),
            "policy_reload_dispatches": len(reload_dispatch),
            "policy_reload_errors": len(reload_errors),
            "comp_lines": len(comp_text.splitlines()),
        }
        print("=== métricas cruas ===")
        print(json.dumps(metrics, indent=2, ensure_ascii=False))
        (OUT / "metrics.json").write_text(json.dumps(metrics, indent=2))
        print(f"artefatos: {OUT}")

        loop = (
            metrics["commit_rate_per_s"] >= 30
            or metrics["panel_state_rate_per_s"] >= 30
            or metrics["frame_configures_total"] > 20
            or metrics["policy_reloads_without_edit"] > 0
        )
        print("VEREDITO: " + ("LOOP" if loop else "IDLE"))
        return 0
    finally:
        for proc in [*feet, runtime, shell]:
            stop(proc)
        stop(comp)


if __name__ == "__main__":
    sys.exit(main())
