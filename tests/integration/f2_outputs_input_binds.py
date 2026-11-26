#!/usr/bin/env python3
"""F2-outputs+input-binds: estado real, atalhos globais e fronteira do guest.

Baseline esperado: api/outputs.ts, api/input.ts e keybindings.ts ausentes.
Pré-requisitos (bench POR ÚLTIMO):
  bun build packages/sdk/src/client.ts --target=browser --outfile=packages/sdk/dist/client.js
  cargo build --workspace --examples
  cargo build -p aeroshell-shell
  cargo build -p aeroshell-compositor --features bench
"""
import json
import os
import pathlib
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-outputs-binds-", dir="/tmp/opencode"))
IPC = OUT / "ipc.sock"
INPUT = OUT / "input.sock"
LOG = OUT / "compositor.log"
HOST_LOG = OUT / "host.log"
RUNTIME_LOG = OUT / "runtime.log"
RUNTIME_DIR = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
SOCKET = f"wayland-aero-f2-binds-{os.getpid()}"
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def wait_for(predicate, timeout=20):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if predicate():
                return True
        except (OSError, ValueError, KeyError, IndexError, json.JSONDecodeError):
            pass
        time.sleep(0.1)
    return False


def command(line, ipc=IPC, dot=False):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.settimeout(6)
        sock.connect(str(ipc))
        sock.sendall((line + "\n").encode())
        lines = []
        for raw in sock.makefile("r", encoding="utf-8"):
            raw = raw.rstrip("\n")
            if raw == "." and dot:
                break
            lines.append(raw)
            if not dot:
                break
        return lines


def stop(proc):
    if proc is None or proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        proc.wait(timeout=6)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait(timeout=5)


def clean(path):
    return ANSI.sub("", path.read_text(errors="replace")) if path.exists() else ""


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def input_chord(number):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.connect(str(INPUT))
        for line in (
            "key press 125", f"key press {number + 1}",
            f"key release {number + 1}", "key release 125",
        ):
            sock.sendall((line + "\n").encode())
            time.sleep(0.08)


def input_repeat_reverse(number):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.connect(str(INPUT))
        lines = ["key press 125"] + [f"key press {number + 1}"] * 3 + [
            "key release 125", f"key release {number + 1}",
        ]
        for line in lines:
            sock.sendall((line + "\n").encode())
            time.sleep(0.08)


def click(x, y):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.connect(str(INPUT))
        for line in (f"motion {x} {y}", "button press", "button release"):
            sock.sendall((line + "\n").encode())
            time.sleep(0.1)


def layer_outputs(namespace):
    marker = f"namespace=aeroshell-surface-{namespace}"
    segments = clean(LOG).split("nova layer surface")
    for segment in segments:
        if marker in segment:
            return re.findall(r"LayerMap após mapear o painel.*?output=\"([^\"]+)\"", segment)
    return []


def guest_ready(surface_id):
    return any(
        "guest-ready" in line and f'\"id\":\"{surface_id}\"' in line
        for line in clean(HOST_LOG).splitlines()
    )


def layer_ready(namespace, output, width, height):
    for line in clean(LOG).splitlines():
        if (
            "LayerMap layer arranjada" in line
            and f"namespace=\"aeroshell-surface-{namespace}\"" in line
            and f"output=\"{output}\"" in line
            and f"width: {width}" in line
            and f"height: {height}" in line
        ):
            return True
    return False


class Events:
    def __init__(self, path):
        self.values = []
        self.lock = threading.Lock()
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(str(path))
        self.thread = threading.Thread(target=self._read, daemon=True)
        self.thread.start()

    def _read(self):
        try:
            for line in self.sock.makefile("r", encoding="utf-8"):
                try:
                    value = json.loads(line)
                except json.JSONDecodeError:
                    continue
                with self.lock:
                    self.values.append(value)
        except OSError:
            pass

    def has(self, kind, predicate=None):
        with self.lock:
            for value in self.values:
                payload = value.get(kind)
                if payload is not None and (predicate is None or predicate(payload)):
                    return True
        return False

    def close(self):
        try:
            self.sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.sock.close()
        self.thread.join(timeout=2)


def base_env(ipc, input_path):
    env = {
        **os.environ,
        "RUST_LOG": "info",
        "AERO_WINDOW_IPC_SOCK": str(ipc),
        "AERO_TEST_INPUT_SOCK": str(input_path),
        "AERO_SURFACE_HOST": "1",
    }
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD", "AERO_DEV_ORIGIN"):
        env.pop(key, None)
    return env


def start_compositor(backend, socket_name, ipc, input_path, log_path, outputs=1, origin=None):
    env = base_env(ipc, input_path)
    if origin is not None:
        env["AERO_DEV_ORIGIN"] = origin
    handle = log_path.open("wb")
    proc = subprocess.Popen(
        [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", backend,
         "--socket", socket_name, "--size", "640x480", "--outputs", str(outputs)],
        cwd=ROOT, env=env, stdout=handle, stderr=subprocess.STDOUT, start_new_session=True,
    )
    if not wait_for(lambda: ipc.exists() and pathlib.Path(f"{ipc}.events").exists()):
        stop(proc)
        raise RuntimeError(f"compositor {backend} não ficou pronto: {log_path}")
    return proc


def preflight():
    source = """
      import { createAero } from './packages/sdk/src/index.ts';
      import './packages/sdk/src/api/outputs.ts';
      import './packages/sdk/src/api/input.ts';
      import './packages/runtime/src/keybindings.ts';
      import { loadDesktopConfig } from './packages/runtime/src/config-loader.ts';
      const config = await loadDesktopConfig('examples/blank-desktop/desktop.config.tsx');
      const panels = (config.surfaces ?? []).filter(s => s.kind === 'Panel');
      if (!createAero || config.keybindings?.length !== 2) throw new Error('dois binds ausentes');
      if (panels.length !== 2 || panels.map(s => s.output).join(',') !== '0,1') {
        throw new Error('blank-desktop precisa de painel nos outputs 0 e 1');
      }
    """
    result = subprocess.run(["bun", "-e", source], cwd=ROOT, capture_output=True, text=True)
    print("=== preflight outputs+input (cru) ===")
    print(result.stdout, end="")
    print(result.stderr, end="")
    return result.returncode == 0


def outputs(ipc=IPC):
    result = []
    for line in command("output-list", ipc, dot=True):
        if line.startswith("O "):
            result.append(json.loads(line[2:]))
    return result


def binds():
    result = []
    for line in command("input-list", dot=True):
        if line.startswith("B "):
            result.append(json.loads(line[2:]))
    return result


def main():
    if not preflight():
        print(f"LOGS: {OUT}")
        print("VEREDITO: FALHA")
        return 1
    origin = f"http://127.0.0.1:{free_port()}"
    fixture = OUT / "fixture"
    fixture.mkdir()
    for name in ("desktop.config.tsx", "panel.ts", "background.ts"):
        shutil.copy(ROOT / "examples/blank-desktop" / name, fixture / name)
    config_path = fixture / "desktop.config.tsx"
    config_path.write_text(config_path.read_text().replace(
        '"../../packages/sdk/src/define"', json.dumps(str(ROOT / "packages/sdk/src/define.ts"))
    ))
    panel_path = fixture / "panel.ts"
    panel_path.write_text(panel_path.read_text().replace(
        '"../../packages/sdk/src/client"', json.dumps(str(ROOT / "packages/sdk/src/client.ts"))
    ))
    comp = host = runtime = callback = control = None
    events = None
    checks = {}
    try:
        control_ipc = OUT / "headless.sock"
        control = start_compositor(
            "headless", f"{SOCKET}-headless", control_ipc, OUT / "headless.input",
            OUT / "headless.log",
        )
        headless = outputs(control_ipc)
        print(f"=== outputs headless (cru): {json.dumps(headless)} ===")
        checks["headless publica nome/geometria/escala/modo"] = (
            len(headless) == 1 and headless[0]["name"] == "aero-headless"
            and headless[0]["geometry"]["width"] == 640
            and headless[0]["scale"] == 1
            and headless[0]["mode"]["refresh_millihz"] == 60_000
        )
        stop(control)
        control = None

        comp = start_compositor("winit", SOCKET, IPC, INPUT, LOG, outputs=2, origin=origin)
        events = Events(pathlib.Path(f"{IPC}.events"))
        env = {**base_env(IPC, INPUT), "AERO_DEV_ORIGIN": origin}
        host_handle = HOST_LOG.open("wb")
        host = subprocess.Popen(
            ["dbus-run-session", "--", str(ROOT / "target/debug/aeroshell-shell")],
            cwd=ROOT, env={**env, "WAYLAND_DISPLAY": SOCKET, "GDK_BACKEND": "wayland",
                           "GTK4_LAYER_SHELL_LIB": "libgtk4-layer-shell.so.0"},
            stdout=host_handle, stderr=subprocess.STDOUT, start_new_session=True,
        )
        checks["host conectou"] = wait_for(lambda: "SURFACE_HOST pronto" in clean(HOST_LOG))
        runtime_handle = RUNTIME_LOG.open("wb")
        runtime = subprocess.Popen(
            ["bun", "packages/runtime/src/main.ts", "--config", str(config_path), "--dev"],
            cwd=ROOT, env=env, stdout=runtime_handle, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        checks["config registra dois binds e três superfícies"] = wait_for(
            lambda: "CONFIG_KEYBINDS count=2" in clean(RUNTIME_LOG)
            and "CONFIG_SURFACES count=3" in clean(RUNTIME_LOG)
        )
        checks["dois outputs winit com posições reais"] = wait_for(
            lambda: len(outputs()) == 2
            and [item["name"] for item in outputs()] == ["aero-0", "aero-1"]
            and [item["geometry"]["x"] for item in outputs()] == [0, 640]
        )
        checks["OutputAdded faz replay dos dois outputs"] = wait_for(
            lambda: events.has("OutputAdded", lambda p: p.get("output", {}).get("name") == "aero-0")
            and events.has("OutputAdded", lambda p: p.get("output", {}).get("name") == "aero-1")
        )
        sdk_source = """
          import { createAero } from './packages/sdk/src/index.ts';
          import { ipcTransport } from './packages/runtime/src/ipc-client.ts';
          const aero = createAero(ipcTransport(process.env.AERO_WINDOW_IPC_SOCK));
          const list = await aero.outputs.list();
          const one = await aero.outputs.get(1);
          console.log('OUTPUT_SDK ' + JSON.stringify({list, one}));
        """
        sdk = subprocess.run(
            ["bun", "-e", sdk_source], cwd=ROOT, env=env, capture_output=True, text=True, timeout=15,
        )
        print(f"=== SDK outputs (cru) ===\n{sdk.stdout}{sdk.stderr}")
        sdk_line = next((line for line in sdk.stdout.splitlines() if line.startswith("OUTPUT_SDK ")), "")
        sdk_data = json.loads(sdk_line.removeprefix("OUTPUT_SDK ")) if sdk_line else {}
        checks["aero.outputs.list/get no IPC real"] = (
            len(sdk_data.get("list", [])) == 2 and sdk_data.get("one", {}).get("name") == "aero-1"
        )
        checks["superfícies nos outputs 0 e 1"] = wait_for(
            lambda: events.has("SurfaceCreated", lambda p: p.get("surface", {}).get("id") == "blank-panel"
                             and p.get("surface", {}).get("output") == 0)
            and events.has("SurfaceCreated", lambda p: p.get("surface", {}).get("id") == "blank-panel-output-1"
                             and p.get("surface", {}).get("output") == 1)
            and "SURFACE_HOST criado id=blank-panel-output-1" in clean(HOST_LOG)
        )
        checks["LayerMap posiciona cada painel somente no output escolhido"] = wait_for(
            lambda: layer_outputs("blank-panel") == ["aero-0"]
            and layer_outputs("blank-panel-output-1") == ["aero-1"]
        )
        checks["geometria do segundo painel está pronta antes do clique"] = wait_for(
            lambda: layer_ready("blank-panel-output-1", "aero-1", 640, 56)
        )
        checks["região de input do segundo painel está pronta antes do clique"] = wait_for(
            lambda: guest_ready("blank-panel-output-1")
        )
        focus_before = clean(LOG).count("TESTE foco de teclado alterado")
        click(960, 450)
        checks["clique no painel do segundo output alcança a layer"] = wait_for(
            lambda: clean(LOG).count("TESTE foco de teclado alterado") > focus_before
            and "is_panel=true" in clean(LOG).split("TESTE foco de teclado alterado")[-1]
        )
        checks["bind/list contém somente os dois grabs declarados"] = wait_for(
            lambda: [item["keys"] for item in binds()] == ["Super+1", "Super+2"]
        )

        created = command("workspace-create Segundo")[0]
        second = int(created.split()[1]) if created.startswith("WORKSPACE ") else 2
        input_chord(2)
        checks["Super+2 dispara ação do SDK"] = wait_for(
            lambda: "KEYBIND_ACTION id=desktop-config-1 command=workspace-activate 2 status=OK"
            in clean(RUNTIME_LOG)
            and any("active=1" in line and line.startswith(f"WS {second} ")
                    for line in command("workspace-list", dot=True))
        )
        checks["KeyBind chega pelo stream real"] = events.has(
            "KeyBind", lambda p: p.get("id") == "desktop-config-1"
        )
        input_chord(1)
        checks["Super+1 volta ao workspace 1"] = wait_for(
            lambda: "KEYBIND_ACTION id=desktop-config-0 command=workspace-activate 1 status=OK"
            in clean(RUNTIME_LOG)
            and any("active=1" in line and line.startswith("WS 1 ")
                    for line in command("workspace-list", dot=True))
        )
        repeat_before = clean(RUNTIME_LOG).count("KEYBIND_ACTION id=desktop-config-0")
        input_repeat_reverse(1)
        time.sleep(0.5)
        checks["repeat e release reverso emitem uma única ação"] = (
            clean(RUNTIME_LOG).count("KEYBIND_ACTION id=desktop-config-0") == repeat_before + 1
        )

        callback_log = OUT / "callback.log"
        callback_source = """
          import { createAero, parseEvent } from './packages/sdk/src/index.ts';
          import { eventStream, ipcTransport } from './packages/runtime/src/ipc-client.ts';
          const path = process.env.AERO_WINDOW_IPC_SOCK;
          const aero = createAero(ipcTransport(path));
          eventStream(path, raw => { const event = parseEvent(raw); if (event) aero.input.dispatch(event); });
          const binding = await aero.input.bind('Super+3', async event => {
            console.log('CALLBACK_OK id=' + event.id);
            await aero.input.unbind(binding.id);
            console.log('CALLBACK_UNBIND id=' + binding.id);
            setTimeout(() => process.exit(0), 50);
          });
          console.log('CALLBACK_READY id=' + binding.id);
          setTimeout(() => process.exit(2), 30000);
        """
        callback_handle = callback_log.open("wb")
        callback = subprocess.Popen(
            ["bun", "-e", callback_source], cwd=ROOT, env=env,
            stdout=callback_handle, stderr=subprocess.STDOUT, start_new_session=True,
        )
        checks["callback privilegiado registrou Super+3"] = wait_for(
            lambda: "CALLBACK_READY" in clean(callback_log)
        )
        input_chord(3)
        checks["callback dispara e unbind remove o grab"] = wait_for(
            lambda: "CALLBACK_OK" in clean(callback_log)
            and "CALLBACK_UNBIND" in clean(callback_log)
            and len(binds()) == 2
        )

        before = clean(RUNTIME_LOG).count("KEYBIND_ACTION id=desktop-config-1")
        config_path.write_text(config_path.read_text() + "\n// reload sem mudar binds\n")
        checks["reload não duplica binds"] = wait_for(
            lambda: "HMR_CONFIG revision=1 status=OK" in clean(RUNTIME_LOG)
            and "KEYBIND_SYNC bind=- unbind=-" in clean(RUNTIME_LOG)
            and len(binds()) == 2
        )
        input_chord(2)
        checks["ação continua única após reload"] = wait_for(
            lambda: clean(RUNTIME_LOG).count("KEYBIND_ACTION id=desktop-config-1") == before + 1
        )

        changed = config_path.read_text().replace(
            '{ keys: "Super+2", command: "workspace-activate 2" }',
            '{ keys: "Super+4", command: "workspace-activate 2" }',
        )
        config_path.write_text(changed)
        checks["update substitui o grab sem duplicar"] = wait_for(
            lambda: "HMR_CONFIG revision=2 status=OK" in clean(RUNTIME_LOG)
            and "KEYBIND_SYNC bind=desktop-config-1 unbind=desktop-config-1" in clean(RUNTIME_LOG)
            and [item["keys"] for item in binds()] == ["Super+1", "Super+4"]
        )
        input_chord(4)
        checks["novo atalho executa a ação atualizada"] = wait_for(
            lambda: clean(RUNTIME_LOG).count("KEYBIND_ACTION id=desktop-config-1") == before + 2
        )
        config_path.write_text(config_path.read_text().replace(
            '    { keys: "Super+4", command: "workspace-activate 2" },\n', "",
        ))
        checks["remove apaga o grab e mantém o restante"] = wait_for(
            lambda: "HMR_CONFIG revision=3 status=OK" in clean(RUNTIME_LOG)
            and "KEYBIND_SYNC bind=- unbind=desktop-config-1" in clean(RUNTIME_LOG)
            and [item["keys"] for item in binds()] == ["Super+1"]
        )

        command("surface-update " + json.dumps({
            "id": "blank-panel", "url": f"{origin}/surfaces/blank-panel/?input-bind-test=1",
        }))
        checks["guest não registra grab global"] = wait_for(
            lambda: "input-bind" in clean(HOST_LOG)
            and "ERR permissão negada" in clean(HOST_LOG)
            and all(item["id"] != "guest-bind" for item in binds())
        )
        checks["compositor e runtime permanecem vivos"] = comp.poll() is None and runtime.poll() is None
        checks["zero erros de protocolo"] = "ProtocolError" not in clean(LOG) + clean(HOST_LOG)
    except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired) as err:
        checks["execução sem exceção"] = False
        print(f"EXCEÇÃO: {err}")
    finally:
        print(f"LOGS: {OUT}")
        print("=== runtime (cru) ===")
        print(clean(RUNTIME_LOG)[-6000:])
        print("=== host (recorte cru) ===")
        for line in clean(HOST_LOG).splitlines():
            if any(token in line for token in ("SURFACE_HOST", "input-bind", "permissão")):
                print(line)
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {label}")
        verdict = "OK" if checks and all(checks.values()) else "FALHA"
        print("VEREDITO: " + verdict)
        if events is not None:
            events.close()
        for proc in (callback, runtime, host, comp, control):
            stop(proc)
    return 0 if verdict == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
