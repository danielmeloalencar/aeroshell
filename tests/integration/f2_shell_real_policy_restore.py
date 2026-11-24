#!/usr/bin/env python3
"""F2-shell-real: teste-first da recomputação do frame após trocar política.

Sintoma do juiz visual (5ª rodada): ao voltar de `None`/`Client` para `Server`,
a moldura reaparece parcial — título decorado, sem borda inferior e com metade
das laterais — especialmente depois de mover/redimensionar a janela.

Hipótese medida: `associate()` pede `wl_surface.frame()` depois de `set_insets`
(linha ~1214 de frame_webview.rs), mas `hide_frame_for()` e `apply_policy_csd()`
mudam insets/slices SEM pedir novo frame callback. O compositor não recomputa o
9-slice com os insets novos até um commit do cliente; o frame fica com a
geometria antiga (canvas crescido no resize).

Medição objetiva: depois de cada transição `Server→None` e `None→Server`, exige
um NOVO `FRAMEWEB: frame_cb_request` (e um `configure` subseqüente). Baseline no
código atual falha; após a correção, passa. A sentença visual final é do humano.

Pré-requisito: cargo build -p aeroshell-compositor --features bench;
cargo build --workspace --examples; cargo build -p aeroshell-shell.
"""
import os
import pathlib
import re
import signal
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNTIME_DIR = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-policy-restore-"))
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
    config = OUT / "config"
    (config / "aeroshell").mkdir(parents=True, exist_ok=True)
    rules = config / "aeroshell" / "rules.ts"
    write_rules(rules, "Server")
    sock = f"wayland-aero-restore-{os.getpid()}"
    ipc = OUT / "ipc.sock"
    comp_log = OUT / "comp.log"
    runtime_log = OUT / "runtime.log"
    env = {**os.environ, "RUST_LOG": "info", "AERO_WINDOW_IPC_SOCK": str(ipc),
           "XDG_CONFIG_HOME": str(config), "HOME": str(OUT)}
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD", "AERO_DECORATION_RULES"):
        env.pop(key, None)

    comp = runtime = foot = None
    try:
        handles = {p: p.open("wb") for p in (comp_log, runtime_log)}
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "winit",
             "--socket", sock,
             "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, env=env, stdout=handles[comp_log], stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        if not wait_for(lambda: ipc.exists() and (RUNTIME_DIR / sock).exists(), 15):
            print("FALHA: compositor não subiu", file=sys.stderr)
            return 1
        runtime = subprocess.Popen(
            ["bun", str(ROOT / "packages/runtime/src/policy.ts")],
            cwd=ROOT, env={**env, "AERO_WINDOW_IPC_SOCK": str(ipc)},
            stdout=handles[runtime_log], stderr=subprocess.STDOUT, start_new_session=True,
        )
        wait_for(lambda: "POLICY_TS READY" in clean(runtime_log), 15)
        foot = subprocess.Popen(
            ["foot"], cwd=ROOT, env={**env, "WAYLAND_DISPLAY": sock},
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True,
        )
        ready = wait_for(
            lambda: "FRAMEWEB: frame_cb_request window_id=0" in clean(comp_log)
            and "POLICY_TS DECISION id=0 app_id=foot policy=Server" in clean(runtime_log),
            20,
        )
        if not ready:
            print("FALHA: frame/decisão inicial não prontos", file=sys.stderr)
            return 1
        time.sleep(1)

        checks = {}
        for policy, marker in (("None", "FRAMEWEB: csd_applied"),
                               ("Server", "FRAMEWEB: csd_restored")):
            before = clean(comp_log).count("FRAMEWEB: frame_cb_request window_id=0")
            before_cfg = clean(comp_log).count("FRAMEWEB: configure serial=")
            write_rules(rules, policy)
            if not wait_for(
                lambda: any(
                    marker in line and "window_id=0" in line
                    for line in clean(comp_log).splitlines()
                ),
                12,
            ):
                print(f"FALHA: transição para {policy} não gerou {marker}", file=sys.stderr)
                return 1
            ok = wait_for(
                lambda: clean(comp_log).count("FRAMEWEB: frame_cb_request window_id=0") > before,
                8,
            )
            after = clean(comp_log).count("FRAMEWEB: frame_cb_request window_id=0")
            # O WebView não pode entregar um frame sem buffer: é o que deixa a
            # moldura invisível mas ainda interativa (juiz visual 8ª rodada).
            lines = clean(comp_log).splitlines()
            idx = [i for i, line in enumerate(lines) if marker in line and "window_id=0" in line]
            no_buffer = 0
            if idx:
                no_buffer = sum(
                    "FRAME_BUFFER_COMMIT buffer=None" in line
                    for line in lines[idx[-1]:]
                )
            checks[f"{policy} entrega buffer pintado"] = no_buffer == 0
            print(f"=== {policy}: commits sem buffer após transição={no_buffer} ===")
            cfg_ok = wait_for(
                lambda: clean(comp_log).count("FRAMEWEB: configure serial=") > before_cfg,
                8,
            )
            after_cfg = clean(comp_log).count("FRAMEWEB: configure serial=")
            checks[f"{policy} pede novo frame callback"] = ok
            checks[f"{policy} recebe novo configure"] = cfg_ok
            print(f"=== {policy}: cb {before}->{after}, configure {before_cfg}->{after_cfg} ===")
            for line in clean(comp_log).splitlines()[-40:]:
                if "FRAMEWEB:" in line and any(
                    token in line for token in ("cb_request", "configure", "csd_", "frame_cb_recv")
                ):
                    print(line[:200])
            time.sleep(1)

        print("=== runtime (cru) ===")
        for line in clean(runtime_log).splitlines():
            if "POLICY_TS" in line:
                print(line[:200])
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {label}")
        print("VEREDITO: " + ("OK" if all(checks.values()) else "FALHA"))
        return 0 if all(checks.values()) else 1
    finally:
        stop(foot)
        stop(runtime)
        stop(comp)


if __name__ == "__main__":
    sys.exit(main())
