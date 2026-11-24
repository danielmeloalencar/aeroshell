#!/usr/bin/env python3
"""F2-shell-real: painel de produção + regras persistentes no config dir.

Sobe o nested com 2 foot, o `aeroshell-shell` real e o runtime Bun lendo
`$XDG_CONFIG_HOME/aeroshell/rules.ts`. Verifica, pelo log do bridge e do
compositor: lista ao vivo (2→3→2), foco por clique, indicadores de foco e
decoração, redecoração de várias janelas ao editar o .ts sem restart, erro de
sintaxe sem efeito, e latência do reload (3 amostras, VM sem 3D).

Baseline (HEAD sem este cartão): sem `panel-state` no log, sem close/indicadores
e o runtime ignora o config dir → VEREDITO: FALHA.

Pre-requisito:
  cargo build -p aeroshell-compositor --features bench
  cargo build --workspace --examples && cargo build -p aeroshell-shell
  bun build packages/panel/src/index.tsx --target=browser --outfile=packages/panel/dist/app.js

Uso: python3 tests/integration/f2_shell_real.py   (AERO_ROOT permite apontar outro checkout)
"""
import json
import os
import pathlib
import socket
import re
import signal
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(os.environ.get("AERO_ROOT", pathlib.Path(__file__).resolve().parents[2]))
RUNTIME_DIR = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-shell-real-"))
ANSI = re.compile(r"\x1b\[[0-9;]*m")


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


def write_rules(path, policy):
    path.write_text(
        'import type { DecorationPolicyConfig } from "'
        + str(ROOT / "packages/sdk/src/generated/DecorationPolicyConfig")
        + '";\n'
        'const config: DecorationPolicyConfig = {\n'
        '  default: "Server",\n'
        '  rules: [{ app_id: "foot", class: null, policy: "' + policy + '" }],\n'
        '};\nexport default config;\n'
    )


def main():
    config_home = OUT / "config"
    (config_home / "aeroshell").mkdir(parents=True, exist_ok=True)
    home = OUT / "home"
    home.mkdir(parents=True, exist_ok=True)
    rules = config_home / "aeroshell" / "rules.ts"
    write_rules(rules, "Server")

    sock = f"wayland-aero-f2-shell-{os.getpid()}"
    ipc = OUT / "ipc.sock"
    input_sock = OUT / "input.sock"
    log = OUT / "comp.log"
    rlog = OUT / "runtime.log"
    env = {**os.environ, "RUST_LOG": "debug", "AERO_WINDOW_IPC_SOCK": str(ipc),
           "AERO_TEST_INPUT_SOCK": str(input_sock),
           # VM sem 3D e muito log: dar folga ao timeout de política (padrão
           # 300 ms) para que um reload mais lento não vire fallback.
           "AERO_POLICY_TIMEOUT_MS": "2000",
           "HOME": str(home), "XDG_CONFIG_HOME": str(config_home)}
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD", "AERO_DECORATION_RULES"):
        env.pop(key, None)

    comp = shell = runtime = extra = None
    feet = []
    try:
        log_handle = log.open("wb")
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "winit",
             "--socket", sock, "--size", "1000x600",
             "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, env=env, stdout=log_handle, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        comp._log_handle = log_handle
        deadline = time.time() + 15
        while time.time() < deadline and not ((RUNTIME_DIR / sock).exists() and ipc.exists()):
            time.sleep(0.05)

        shell_env = {**env, "WAYLAND_DISPLAY": sock, "GDK_BACKEND": "wayland",
                     "GTK4_LAYER_SHELL_LIB": os.environ.get("GTK4_LAYER_SHELL_LIB", "libgtk4-layer-shell.so.0")}
        shell = subprocess.Popen(
            ["dbus-run-session", "--", str(ROOT / "target/debug/aeroshell-shell")],
            cwd=ROOT, env=shell_env, stdout=log_handle, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        run_env = {**env, "AERO_WINDOW_IPC_SOCK": str(ipc)}
        runtime = subprocess.Popen(
            ["bun", str(ROOT / "packages/runtime/src/policy.ts")],
            cwd=ROOT, env=run_env, stdout=rlog.open("wb"), stderr=subprocess.STDOUT,
            start_new_session=True,
        )

        def clean(path):
            text = path.read_text(errors="ignore") if path.exists() else ""
            return ANSI.sub("", text)

        def panel_state():
            for raw in reversed(re.findall(r"mensagem da WebView json=(.+)$", clean(log), re.M)):
                try:
                    value = json.loads(raw)
                except json.JSONDecodeError:
                    continue
                if isinstance(value, str):
                    try:
                        value = json.loads(value)
                    except json.JSONDecodeError:
                        continue
                if isinstance(value, dict) and value.get("type") == "panel-state":
                    return value
            return None

        def rows():
            state = panel_state()
            return state["rows"] if state else []

        def close_rects():
            state = panel_state()
            return state["closes"] if state else {}

        def geometry():
            # Mesma sonda de react_panel.py: posição do painel na saída. O
            # `LayerMap` dá a zona exclusiva (0,0), não a origem do layer surface.
            panel_lines = [
                line for line in clean(log).splitlines()
                if "geometria real da layer surface" in line
            ]
            found = re.findall(
                r"geometry\s*=\s*Some\(Rectangle<[^>]*>\s*\{\s*x:\s*(-?\d+),\s*y:\s*(-?\d+)",
                "\n".join(panel_lines),
            )
            return tuple(map(int, found[-1])) if found else (0, 0)

        def panel_rects():
            matches = re.findall(r"rects\s*=\s*\[([^\]]*)\]", clean(log))
            return [
                dict(zip(("x", "y", "w", "h"), map(int, parts)))
                for parts in re.findall(r"\((\d+),\s*(\d+),\s*(\d+),\s*(\d+)\)", matches[-1])
            ] if matches else []

        def render_order():
            lines = re.findall(r"ordem de render \(frente->tras\) order=(.+)$", clean(log), re.M)
            return lines[-1] if lines else None

        def click(x, y):
            sock_input = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            try:
                sock_input.connect(str(input_sock))
                for line in (f"motion {x} {y}", "button press", "button release"):
                    sock_input.sendall((line + "\n").encode())
                    time.sleep(0.3)
            finally:
                sock_input.close()

        def decisions():
            return {
                wid: policy.lower()
                for wid, policy in re.findall(
                    r"POLICY_TS DECISION id=(\d+) app_id=\S+ policy=(\w+)", clean(rlog)
                )
            }

        def decision_count():
            return len(re.findall(r"POLICY_TS DECISION id=", clean(rlog)))

        def suppressed():
            """Último estado de POLICY_RENDER por janela (não 'já foi suprimida')."""
            last = {}
            for wid, flag in re.findall(
                r"POLICY_RENDER t=\d+ window_id=(\d+) suppressed=(\w+)", clean(log)
            ):
                last[wid] = flag
            return {wid for wid, flag in last.items() if flag == "policy_none"}

        def dispatch_count():
            return len(re.findall(r"POLICY_TS RELOAD_DISPATCH", clean(rlog)))

        if not wait_for(lambda: "POLICY_TS READY" in clean(rlog), 15):
            print("FALHA: runtime não anunciou policy-ready", file=sys.stderr)
            return 1

        # Clients DEPOIS do runtime: no headless os primeiros foot mapeavam
        # antes do policy-ready e ficavam sem decisão explícita.
        for _ in range(2):
            feet.append(subprocess.Popen(
                ["foot"], cwd=ROOT, env={**env, "WAYLAND_DISPLAY": sock},
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                start_new_session=True,
            ))

        # --- cenário -------------------------------------------------------
        checks = {}
        checks["runtime leu regras do config dir"] = (
            "POLICY_TS READY" in clean(rlog)
            and "source=configdir" in clean(rlog)
            and "POLICY_TS RULES source=configdir" in clean(rlog)
        )
        if not wait_for(lambda: len(rows()) == 2, 15):
            print("FALHA: painel não listou 2 janelas; viu", rows(), file=sys.stderr)
            return 1
        checks["painel lista 2 foot ao vivo"] = len(rows()) == 2
        checks["sem polling no bundle"] = "setInterval" not in (
            ROOT / "packages/panel/dist/app.js"
        ).read_text(errors="ignore")
        checks["indicador de decoração no estado inicial"] = all(
            r.get("decoration") == "Server" for r in rows()
        )
        comp_pid, runtime_pid = comp.pid, runtime.pid

        extra = subprocess.Popen(
            ["foot"], cwd=ROOT, env={**env, "WAYLAND_DISPLAY": sock},
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True,
        )
        checks["3ª janela aparece ao vivo"] = wait_for(lambda: len(rows()) == 3, 12)

        # Foco: clicar numa linha AINDA não focada muda o indicador na mesma
        # linha (evita depender do índice de wl_surface no render order).
        rects = panel_rects()
        gx, gy = geometry()
        unfocused = [r for r in rows() if not r.get("focused")]
        focus_target = unfocused[0] if unfocused else rows()[0]
        focus_index = next(
            (i for i, r in enumerate(rows()) if r["id"] == focus_target["id"]), 0
        )
        if focus_index < len(rects):
            r = rects[focus_index]
            click(gx + int(r["x"]) + int(r["w"]) // 2, gy + int(r["y"]) + int(r["h"]) // 2)
        checks["foco por clique"] = wait_for(
            lambda: any(
                row.get("focused") and row["id"] == focus_target["id"] for row in rows()
            ),
            8,
        )
        print(f"foco: alvo={focus_target['id']} antes={render_order()} depois={render_order()} rects={rects}")

        # Fechar: o ✕ fica na borda direita da linha; usar a geometria da linha
        # publicada no `interactive` (o rect do span é 0 enquanto o CSS não
        # reserva largura para ele) e clicar 8 px antes da borda direita.
        target = rows()[-1]["id"]
        wait_for(lambda: str(target) in close_rects(), 8)
        close_rect = close_rects().get(str(target))
        if close_rect is not None:
            click(
                gx + int(close_rect["x"]) + int(close_rect["w"]) // 2,
                gy + int(close_rect["y"]) + int(close_rect["h"]) // 2,
            )
        checks["✕ fecha só aquela janela"] = wait_for(
            lambda: len(rows()) == 2 and all(row["id"] != target for row in rows()), 25
        )

        # Editar regra no config dir: todas as janelas restantes mudam juntas.
        latencies = []

        def reload_to(policy):
            before_dispatch = dispatch_count()
            t0 = time.time()
            write_rules(rules, policy)
            dispatched = wait_for(lambda: dispatch_count() > before_dispatch, 10)
            if dispatched:
                latencies.append((time.time() - t0) * 1000)
            wait_for(
                lambda: all(p == policy.lower() for p in decisions().values()), 10
            )
            # O reload limpa `pending_policies` e reemite; responder vários
            # reloads em sequência apertada gera "janela sem pedido pendente".
            time.sleep(1.5)
            return dispatched

        reload_to("None")
        checks["indicador acompanha a edição"] = all(
            row.get("decoration") == "None" for row in rows()
        )
        live_ids = {str(row["id"]) for row in rows()}
        checks["múltiplas janelas redecoradas sem restart"] = (
            len(live_ids) == 2
            and all(decisions().get(wid) == "none" for wid in live_ids)
            and live_ids.issubset(suppressed())
        )
        reload_to("Server")
        # A sonda POLICY_RENDER só imprime quando SUPRIME (não há linha
        # `suppressed=none`): o estado atual é medido pelas decisões do runtime
        # e pelo indicador do painel, não pelo histórico da sonda.
        checks["volta a Server sem restart"] = wait_for(
            lambda: all(decisions().get(wid) == "server" for wid in live_ids)
            and all(row.get("decoration") == "Server" for row in rows()),
            10,
        )
        reload_to("None")
        reload_to("Server")
        checks["hot-reload sem restart"] = (
            len(latencies) >= 3 and comp.pid == comp_pid and runtime.pid == runtime_pid
        )
        print("latências_ms:", [round(v, 1) for v in latencies])

        # Erro de sintaxe: última boa preservada, nenhuma decisão nova.
        before_decisions = decision_count()
        rules.write_text("export default { default: ; syntax error };\n")
        wait_for(lambda: "POLICY_TS RELOAD ERROR" in clean(rlog), 8)
        time.sleep(1)
        checks["erro de sintaxe sem efeito"] = (
            decision_count() == before_decisions
            and runtime.poll() is None
            and all(row.get("decoration") == "Server" for row in rows())
        )

        logs = clean(log)
        rlogs = clean(rlog)
        print("=== runtime (cru, recortado) ===")
        for line in rlogs.splitlines():
            if "POLICY_TS" in line:
                print(line)
        print("=== compositor (cru, recortado) ===")
        for line in logs.splitlines():
            if "POLICY" in line or "BRIDGE aeroEvent" in line or "ordem de render" in line:
                print(line[:240])

        checks["zero erros de protocolo"] = (
            "ProtocolError" not in logs and "reason=timeout" not in rlogs + logs
        )
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {label}")
        print("VEREDITO: " + ("OK" if all(checks.values()) else "FALHA"))
        return 0 if all(checks.values()) else 1
    finally:
        for proc in [*feet, extra, runtime, shell]:
            stop(proc)
        stop(comp)
        handle = getattr(comp, "_log_handle", None) if comp is not None else None
        if handle is not None:
            handle.close()


if __name__ == "__main__":
    sys.exit(main())
