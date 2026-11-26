#!/usr/bin/env python3
"""F2-windows-completo: work area por output, estados, IPC+SDK e grabs.

Baseline esperado: métodos aero.windows ausentes -> VEREDITO: FALHA.
Pré-requisitos (bench POR ÚLTIMO):
  cargo build --workspace --examples
  cargo build -p aeroshell-shell
  cargo build -p aeroshell-compositor --example toplevel_states_client
  cargo build -p aeroshell-compositor --features bench
"""
import json
import os
import pathlib
import re
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNTIME = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-windows-", dir="/tmp/opencode"))
IPC = OUT / "ipc.sock"
INPUT = OUT / "input.sock"
LOG = OUT / "compositor.log"
HOST_LOG = OUT / "host.log"
RUNTIME_LOG = OUT / "runtime.log"
CLIENT_LOG = OUT / "client.log"
SOCKET = f"wayland-aero-f2-windows-{os.getpid()}"
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def clean(path):
    return ANSI.sub("", path.read_text(errors="replace")) if path.exists() else ""


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


def command(line, dot=False):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.settimeout(6)
        sock.connect(str(IPC))
        sock.sendall((line + "\n").encode())
        lines = []
        for raw in sock.makefile("r", encoding="utf-8"):
            raw = raw.rstrip("\n")
            lines.append(raw)
            if (dot and raw == ".") or (not dot and raw.startswith(("OK", "ERR"))):
                break
        return lines


def windows():
    result = {}
    for line in command("list", dot=True):
        match = re.match(
            r"W (\d+) (-?\d+) (-?\d+) (\d+) (\d+) (\"(?:[^\"\\]|\\.)*\") .*mode=(\w+)",
            line,
        )
        if match:
            result[int(match[1])] = {
                "x": int(match[2]), "y": int(match[3]),
                "width": int(match[4]), "height": int(match[5]),
                "title": json.loads(match[6]), "mode": match[7],
            }
    return result


def workspaces():
    return command("workspace-list", dot=True)


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


def pointer(*lines):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.connect(str(INPUT))
        for line in lines:
            sock.sendall((line + "\n").encode())
            time.sleep(0.12)


def sdk(expression, timeout=20, raw=False):
    source = expression if raw else f"""
      import {{ createAero }} from './packages/sdk/src/index.ts';
      import {{ ipcTransport }} from './packages/runtime/src/ipc-client.ts';
      const aero = createAero(ipcTransport(process.env.AERO_WINDOW_IPC_SOCK));
      const result = await (async () => {{ {expression} }})();
      if (result !== undefined) console.log('SDK_RESULT ' + JSON.stringify(result));
    """
    result = subprocess.run(
        ["bun", "-e", source], cwd=ROOT,
        env={**os.environ, "AERO_WINDOW_IPC_SOCK": str(IPC)},
        capture_output=True, text=True, timeout=timeout,
    )
    print(f"=== SDK {expression} (cru) ===\n{result.stdout}{result.stderr}")
    return result


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

    def has(self, kind, **fields):
        with self.lock:
            for value in self.values:
                payload = value.get(kind)
                if isinstance(payload, dict) and all(payload.get(k) == v for k, v in fields.items()):
                    return True
        return False

    def close(self):
        try:
            self.sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.sock.close()
        self.thread.join(timeout=2)


def preflight():
    source = """
      import { createAero } from './packages/sdk/src/index.ts';
      const aero = createAero(async () => ['OK']);
      const required = ['minimize', 'maximize', 'fullscreen', 'move', 'resize', 'beginMove', 'beginResize', 'thumbnail'];
      const missing = required.filter(name => typeof aero.windows[name] !== 'function');
      if (missing.length) throw new Error('aero.windows ausentes: ' + missing.join(','));
    """
    result = subprocess.run(["bun", "-e", source], cwd=ROOT, capture_output=True, text=True)
    print("=== preflight aero.windows (cru) ===")
    print(result.stdout, end="")
    print(result.stderr, end="")
    return result.returncode == 0


def latest_visibility(window_id):
    values = re.findall(
        rf"WORKSPACE_RENDER[^\n]*window_id={window_id}[^\n]*visible=(true|false)",
        clean(LOG),
    )
    return values[-1] == "true" if values else None


def main():
    if not preflight():
        print(f"LOGS: {OUT}")
        print("VEREDITO: FALHA")
        return 1
    comp = host = runtime = client = foot = None
    events = None
    checks = {}
    try:
        env = {
            **os.environ, "RUST_LOG": "info",
            "AERO_WINDOW_IPC_SOCK": str(IPC), "AERO_TEST_INPUT_SOCK": str(INPUT),
            "AERO_SURFACE_HOST": "1",
        }
        for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD", "AERO_DEV_ORIGIN"):
            env.pop(key, None)
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "winit",
             "--socket", SOCKET, "--size", "640x480", "--outputs", "2",
             "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, env=env, stdout=LOG.open("wb"), stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        checks["compositor e IPC prontos"] = wait_for(
            lambda: (RUNTIME / SOCKET).exists() and IPC.exists() and pathlib.Path(f"{IPC}.events").exists()
        )
        if not checks["compositor e IPC prontos"]:
            raise RuntimeError("compositor não ficou pronto")
        events = Events(pathlib.Path(f"{IPC}.events"))
        host = subprocess.Popen(
            ["dbus-run-session", "--", str(ROOT / "target/debug/aeroshell-shell")],
            cwd=ROOT, env={**env, "WAYLAND_DISPLAY": SOCKET, "GDK_BACKEND": "wayland",
                           "GTK4_LAYER_SHELL_LIB": "libgtk4-layer-shell.so.0"},
            stdout=HOST_LOG.open("wb"), stderr=subprocess.STDOUT, start_new_session=True,
        )
        checks["host genérico pronto"] = wait_for(lambda: "SURFACE_HOST pronto" in clean(HOST_LOG))
        runtime = subprocess.Popen(
            ["bun", "packages/runtime/src/main.ts", "--config",
             "examples/blank-desktop/desktop.config.tsx"],
            cwd=ROOT, env=env, stdout=RUNTIME_LOG.open("wb"), stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        checks["dois painéis exclusivos prontos"] = wait_for(
            lambda: "CONFIG_SURFACES count=3" in clean(RUNTIME_LOG)
            and clean(LOG).count("LayerMap layer arranjada") >= 3
            and "height: 56" in clean(LOG)
        )
        client = subprocess.Popen(
            [str(ROOT / "target/debug/examples/toplevel_states_client")],
            cwd=ROOT, env={**os.environ, "WAYLAND_DISPLAY": SOCKET},
            stdin=subprocess.PIPE, stdout=CLIENT_LOG.open("wb"), stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        checks["toplevel e frame mapeados"] = wait_for(
            lambda: 0 in windows() and "FRAMEWEB: associate(window_id=0)" in clean(LOG)
        )
        if not all(checks.values()):
            raise RuntimeError("setup incompleto; veja os logs")

        result = sdk("await aero.windows.resize(0, 500, 300);")
        checks["SDK resize configura 500x300"] = result.returncode == 0 and wait_for(
            lambda: windows().get(0, {}).get("width") == 500
            and windows().get(0, {}).get("height") == 300
        )
        result = sdk("await aero.windows.move(0, 700, 100);")
        checks["SDK move posiciona no output 1 e publica Geometry"] = result.returncode == 0 and wait_for(
            lambda: windows().get(0, {}).get("x") == 700
            and windows().get(0, {}).get("y") == 100
            and events.has("Geometry", id=0, x=700, y=100)
        )

        result = sdk("await aero.windows.maximize(0);")
        maximized = windows().get(0, {})
        checks["maximize usa work area do output 1 menos painel/insets"] = (
            result.returncode == 0
            and wait_for(lambda: windows().get(0, {}).get("mode") == "maximized"
                         and windows().get(0, {}).get("x") == 646
                         and windows().get(0, {}).get("y") == 30
                         and windows().get(0, {}).get("width") == 628
                         and windows().get(0, {}).get("height") == 388)
            and events.has("Maximized", id=0, maximized=True)
        )
        print(f"=== maximize output 1 (cru): {json.dumps(maximized)} ===")
        sdk("await aero.windows.maximize(0);")
        checks["unmaximize restaura antes dos demais estados"] = wait_for(
            lambda: windows().get(0, {}).get("mode") == "normal"
            and windows().get(0, {}).get("x") == 700
            and windows().get(0, {}).get("width") == 500
        )

        result = sdk("await aero.windows.minimize(0);")
        checks["minimize esconde e publica Minimized"] = result.returncode == 0 and wait_for(
            lambda: events.has("Minimized", id=0, minimized=True)
            and latest_visibility(0) is False
        )
        checks["minimize preserva membership/workspace"] = any(
            line.startswith("WS 1 ") and "windows=0 " in line for line in workspaces()
        )
        result = sdk("await aero.windows.focus(0);")
        checks["focus restaura minimizada sem recriar janela"] = result.returncode == 0 and wait_for(
            lambda: events.has("Minimized", id=0, minimized=False)
            and latest_visibility(0) is True and 0 in windows()
        )

        sdk("await aero.windows.move(0, 700, 100);")
        result = sdk("await aero.windows.fullscreen(0);")
        checks["fullscreen cobre somente o output sob a janela"] = (
            result.returncode == 0
            and wait_for(lambda: windows().get(0, {}).get("mode") == "fullscreen"
                         and windows().get(0, {}).get("x") == 640
                         and windows().get(0, {}).get("y") == 0
                         and windows().get(0, {}).get("width") == 640
                         and windows().get(0, {}).get("height") == 480)
            and events.has("Fullscreen", id=0, fullscreen=True)
        )
        sdk("await aero.windows.fullscreen(0);")
        checks["unfullscreen restaura geometria"] = wait_for(
            lambda: windows().get(0, {}).get("mode") == "normal"
            and windows().get(0, {}).get("x") == 700
            and windows().get(0, {}).get("width") == 500
        )

        sdk("await aero.windows.move(0, 700, 100);")
        pointer("motion 750 150", "button press")
        result = sdk("await aero.windows.beginMove(0);")
        pointer("motion 900 250", "button release")
        checks["beginMove entrega grab Rust ao ponteiro"] = result.returncode == 0 and wait_for(
            lambda: windows().get(0, {}).get("x") == 850
            and windows().get(0, {}).get("y") == 200
        )
        pointer("button press")
        result = sdk("await aero.windows.beginResize(0, 'BottomRight');")
        pointer("motion 960 310", "button release")
        checks["beginResize entrega grab Rust e aplica delta"] = result.returncode == 0 and wait_for(
            lambda: windows().get(0, {}).get("width") == 560
            and windows().get(0, {}).get("height") == 360
        )

        result = sdk("await aero.windows.thumbnail(0);")
        checks["thumbnail falha explicitamente para F7"] = (
            result.returncode != 0 and "F7" in result.stderr and "screencopy" in result.stderr
        )

        store_source = """
          import { createAero, parseEvent } from './packages/sdk/src/index.ts';
          import { eventStream, ipcTransport } from './packages/runtime/src/ipc-client.ts';
          const path = process.env.AERO_WINDOW_IPC_SOCK;
          const aero = createAero(ipcTransport(path));
          await aero.windows.list();
          const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
          let replayed = false;
          const disconnect = eventStream(path, raw => {
            const event = parseEvent(raw);
            if (event) aero.store.applyEvent(event);
            if (event && 'Snapshot' in event) replayed = true;
          });
          for (let i = 0; i < 100 && !replayed; i++) await sleep(50);
          await aero.windows.minimize(0);
          for (let i = 0; i < 100 && !aero.store.getWindowState(0).minimized; i++) await sleep(50);
          console.log('STORE_MINIMIZED ' + JSON.stringify(aero.store.getWindowState(0)));
          await aero.windows.focus(0);
          for (let i = 0; i < 100 && aero.store.getWindowState(0).minimized; i++) await sleep(50);
          console.log('STORE_RESTORED ' + JSON.stringify(aero.store.getWindowState(0)));
          disconnect();
        """
        store = sdk(store_source, raw=True)
        checks["store reativo acompanha stream real"] = (
            store.returncode == 0
            and '"minimized":true' in store.stdout
            and '"minimized":false' in store.stdout
        )
        # O cliente sintético não cobre a grade/commit do foot real (lacuna vista
        # pelo juiz): repita maximize/fullscreen com um segundo cliente, agora foot.
        foot = subprocess.Popen(
            ["foot", "-w", "500x300", "-T", "foot F2-windows"],
            cwd=ROOT, env={**os.environ, "WAYLAND_DISPLAY": SOCKET, "GDK_BACKEND": "wayland"},
            stdout=(OUT / "foot.log").open("wb"), stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        foot_id = 1
        checks["foot real mapeado para cenário multi-output"] = wait_for(
            lambda: foot_id in windows() and "FRAMEWEB: title_applied window_id=1" in clean(LOG)
        )
        sdk(f"await aero.windows.resize({foot_id}, 500, 300);")
        checks["foot real aceita resize antes de mover"] = wait_for(
            lambda: windows().get(foot_id, {}).get("width", 0) <= 500
            and windows().get(foot_id, {}).get("height", 0) <= 300
        )
        sdk(f"await aero.windows.move({foot_id}, 700, 100);")
        checks["foot real fica sob o centro do output 1"] = wait_for(
            lambda: windows().get(foot_id, {}).get("x") == 700
            and windows().get(foot_id, {}).get("y") == 100
        )
        sdk(f"await aero.windows.maximize({foot_id});")
        checks["foot real maximiza no output 1"] = wait_for(
            lambda: windows().get(foot_id, {}).get("mode") == "maximized"
            and windows().get(foot_id, {}).get("x") == 646
            and windows().get(foot_id, {}).get("y") == 30
            and windows().get(foot_id, {}).get("width") == 628
            and windows().get(foot_id, {}).get("height") == 388
        )
        sdk(f"await aero.windows.maximize({foot_id});")
        checks["foot real restaura maximize"] = wait_for(
            lambda: windows().get(foot_id, {}).get("mode") == "normal"
        )
        sdk(f"await aero.windows.fullscreen({foot_id});")
        checks["foot real entra em fullscreen no output 1"] = wait_for(
            lambda: windows().get(foot_id, {}).get("mode") == "fullscreen"
            and windows().get(foot_id, {}).get("x") == 640
            and windows().get(foot_id, {}).get("y") == 0
            and windows().get(foot_id, {}).get("width") == 640
            and windows().get(foot_id, {}).get("height") == 480
        )
        checks["compositor/runtime permanecem vivos"] = comp.poll() is None and runtime.poll() is None
        checks["zero erros de protocolo"] = "ProtocolError" not in clean(LOG) + clean(HOST_LOG)
    except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired) as err:
        checks["execução sem exceção"] = False
        print(f"EXCEÇÃO: {err}")
    finally:
        print(f"LOGS: {OUT}")
        print("=== janelas/workspace finais (cru) ===")
        try:
            print(json.dumps(windows()))
            print("\n".join(workspaces()))
        except OSError:
            pass
        print("=== eventos de janela (cru) ===")
        if events is not None:
            with events.lock:
                for value in events.values:
                    if any(key in value for key in ("Minimized", "Maximized", "Fullscreen", "Geometry")):
                        print(json.dumps(value))
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {label}")
        verdict = "OK" if checks and all(checks.values()) else "FALHA"
        print("VEREDITO: " + verdict)
        if client is not None and client.stdin is not None and client.poll() is None:
            try:
                client.stdin.write(b"quit\n")
                client.stdin.flush()
            except OSError:
                pass
        if events is not None:
            events.close()
        for proc in (foot, client, runtime, host, comp):
            stop(proc)
    return 0 if verdict == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
