#!/usr/bin/env python3
"""F2-launcher: stream, SDK Bun real, workspaces e painel React.

Baseline esperado antes do cartão: AppListUpdated/listApps/launch e os controles
`.aero-launcher-*` não existem → VEREDITO: FALHA.

Pré-requisitos (bench DEPOIS do check.sh):
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-shell --examples
  cargo build -p aeroshell-shell
  bun build packages/panel/src/index.tsx --target=browser --outfile=packages/panel/dist/app.js

Uso: python3 tests/integration/f2_launcher.py
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
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-launcher-", dir="/tmp/opencode"))
IPC = OUT / "ipc.sock"
INPUT = OUT / "input.sock"
LOG = OUT / "compositor.log"
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def wait_for(predicate, timeout=12):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if predicate():
                return True
        except (KeyError, IndexError, TypeError, OSError, json.JSONDecodeError):
            pass
        time.sleep(0.05)
    return False


def command(line, multiline=False):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.settimeout(5)
        sock.connect(str(IPC))
        sock.sendall((line + "\n").encode())
        stream = sock.makefile("r", encoding="utf-8")
        lines = []
        while True:
            current = stream.readline()
            if not current:
                break
            current = current.rstrip("\n")
            lines.append(current)
            if not multiline or current == "." or current.startswith(("OK", "ERR")):
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


def clean(path=LOG):
    return ANSI.sub("", path.read_text(errors="replace")) if path.exists() else ""


def window_ids():
    return [int(line.split()[1]) for line in command("list", multiline=True) if line.startswith("W ")]


def workspaces():
    result = {}
    for line in command("workspace-list", multiline=True):
        match = re.match(r"WS (\d+) active=([01]) windows=([0-9,]*) (.*)$", line)
        if match:
            result[int(match.group(1))] = {
                "active": match.group(2) == "1",
                "windows": [int(value) for value in match.group(3).split(",") if value],
            }
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

    def app_lists(self):
        with self.lock:
            return [value["AppListUpdated"]["apps"] for value in self.values if "AppListUpdated" in value]

    def kinds(self):
        with self.lock:
            return [next(iter(value), "") for value in self.values]

    def close(self):
        try:
            self.sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.sock.close()
        self.thread.join(timeout=2)


def panel_state():
    for raw in reversed(re.findall(r"mensagem da WebView json=(.+)$", clean(), re.M)):
        try:
            value = json.loads(raw)
            if isinstance(value, str):
                value = json.loads(value)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and value.get("type") == "panel-state":
            return value
    return None


def panel_geometry():
    found = re.findall(
        r"geometry\s*=\s*Some\(Rectangle<[^>]*>\s*\{\s*x:\s*(-?\d+),\s*y:\s*(-?\d+)",
        clean(),
    )
    return tuple(map(int, found[-1])) if found else (0, 0)


def click(rect):
    if not rect:
        return False
    x, y = panel_geometry()
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.connect(str(INPUT))
        for line in (
            f"motion {x + int(rect['x']) + int(rect['w']) // 2} "
            f"{y + int(rect['y']) + int(rect['h']) // 2}",
            "button press",
            "button release",
        ):
            sock.sendall((line + "\n").encode())
            time.sleep(0.2)
    return True


def type_text(text):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.connect(str(INPUT))
        sock.sendall(("text " + text + "\n").encode())
    time.sleep(0.2)


def main():
    apps = OUT / "apps"
    apps.mkdir()
    (apps / "aero-test-gedit.desktop").write_text(
        "[Desktop Entry]\nType=Application\nName=gedit\n"
        "Exec=foot -T aero-launcher-gedit %U\nIcon=utilities-terminal\n"
        "Categories=Utility;TextEditor;\nStartupWMClass=foot\n",
        encoding="utf-8",
    )
    (apps / "aero-test-calculator.desktop").write_text(
        "[Desktop Entry]\nType=Application\nName=Calculadora\n"
        "Exec=foot -T nao-deve-abrir\nIcon=accessories-calculator\nCategories=Utility;\n",
        encoding="utf-8",
    )
    (apps / "hidden.desktop").write_text(
        "[Desktop Entry]\nType=Application\nName=Oculto\nExec=true\nNoDisplay=true\n",
        encoding="utf-8",
    )

    wayland = f"wayland-aero-f2-launcher-{os.getpid()}"
    env = {
        **os.environ,
        "RUST_LOG": "debug",
        "AERO_WINDOW_IPC_SOCK": str(IPC),
        "AERO_TEST_INPUT_SOCK": str(INPUT),
        "AERO_APP_DIRS": str(apps),
    }
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
        env.pop(key, None)
    comp = shell = None
    events = None
    checks = {}
    try:
        handle = LOG.open("wb")
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "winit",
             "--socket", wayland, "--size", "1000x600",
             "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, env=env, stdout=handle, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        checks["compositor e IPC prontos"] = wait_for(
            lambda: (RUNTIME / wayland).exists() and IPC.exists() and pathlib.Path(f"{IPC}.events").exists(),
            15,
        )
        if not checks["compositor e IPC prontos"]:
            raise RuntimeError("compositor/IPC não ficou pronto")
        events = Events(pathlib.Path(f"{IPC}.events"))
        checks["AppListUpdated contém os 2 apps visíveis"] = wait_for(
            lambda: events.app_lists() and sorted(app["id"] for app in events.app_lists()[-1])
            == ["aero-test-calculator", "aero-test-gedit"]
        )

        shell = subprocess.Popen(
            ["dbus-run-session", "--", str(ROOT / "target/debug/aeroshell-shell")],
            cwd=ROOT,
            env={**env, "WAYLAND_DISPLAY": wayland, "GDK_BACKEND": "wayland",
                 "GTK4_LAYER_SHELL_LIB": os.environ.get("GTK4_LAYER_SHELL_LIB", "libgtk4-layer-shell.so.0")},
            stdout=handle, stderr=subprocess.STDOUT, start_new_session=True,
        )
        created = command("workspace-create Trabalho")[0]
        second = int(re.match(r"WORKSPACE (\d+)", created).group(1))

        sdk_script = (
            'import { createAero, parseEvent } from "' + str(ROOT / "packages/sdk/src/index.ts") + '";\n'
            'import { ipcTransport, eventStream } from "' + str(ROOT / "packages/runtime/src/ipc-client.ts") + '";\n'
            'const path = process.env.AERO_WINDOW_IPC_SOCK;\n'
            'const aero = createAero(ipcTransport(path));\n'
            'const close = eventStream(path, raw => { const event = parseEvent(raw); if (event) aero.store.applyEvent(event); });\n'
            'const deadline = Date.now() + 5000;\n'
            'while (aero.listApps().length < 2 && Date.now() < deadline) await new Promise(r => setTimeout(r, 25));\n'
            'const names = aero.listApps().map(app => app.name).sort();\n'
            'const fuzzy = aero.searchApps("ged").map(app => app.id);\n'
            'const category = aero.searchApps("text").map(app => app.id);\n'
            'const empty = aero.searchApps("app inexistente");\n'
            'if (names.join(",") !== "Calculadora,gedit") throw new Error("lista=" + JSON.stringify(names));\n'
            'if (fuzzy.join(",") !== "aero-test-gedit") throw new Error("fuzzy=" + JSON.stringify(fuzzy));\n'
            'if (category.join(",") !== "aero-test-gedit") throw new Error("categoria=" + JSON.stringify(category));\n'
            'if (empty.length !== 0) throw new Error("busca vazia=" + JSON.stringify(empty));\n'
            f'aero.launchApp("aero-test-gedit", {second});\n'
            'await new Promise(r => setTimeout(r, 500)); close();\n'
            'console.log("SDK_APPS OK " + JSON.stringify({names, fuzzy, category, empty}));\n'
        )
        sdk_run = subprocess.run(
            ["bun", "-e", sdk_script], cwd=ROOT, env=env,
            capture_output=True, text=True, timeout=20,
        )
        print("=== SDK Bun real (cru) ===")
        print(sdk_run.stdout, end="")
        print(sdk_run.stderr, end="")
        checks["SDK lista, busca por nome/categoria e vazio"] = "SDK_APPS OK" in sdk_run.stdout
        checks["SDK lança no workspace especificado"] = wait_for(
            lambda: len(workspaces().get(second, {}).get("windows", [])) == 1
            and workspaces()[second]["active"]
        )

        command("workspace-activate 1")
        raw_launch = command("launch aero-test-gedit")[0]
        print(f"=== launch app já aberto (cru): {raw_launch} ===")
        first_ids = window_ids()
        checks["launch foca app existente sem segunda instância"] = wait_for(
            lambda: workspaces().get(second, {}).get("active") is True
            and window_ids() == first_ids and len(first_ids) == 1
        )

        for window_id in first_ids:
            command(f"close {window_id}")
        checks["fixture fechada antes do teste do painel"] = wait_for(lambda: window_ids() == [])
        command("workspace-activate 1")
        checks["painel publica launcher fechado"] = wait_for(
            lambda: (panel_state() or {}).get("launcher", {}).get("open") is False
        )
        state = panel_state() or {}
        checks["clique abre launcher e campo de busca"] = click(state.get("launcher", {}).get("toggleRect")) and wait_for(
            lambda: (panel_state() or {}).get("launcher", {}).get("open") is True
            and (panel_state() or {})["launcher"].get("searchRect")
        )
        checks["launcher mostra ícones e os 2 apps"] = wait_for(
            lambda: sorted(app["id"] for app in (panel_state() or {}).get("launcher", {}).get("apps", []))
            == ["aero-test-calculator", "aero-test-gedit"]
            and all(app.get("icon") for app in (panel_state() or {})["launcher"]["apps"])
        )
        state = panel_state() or {}
        checks["campo de busca recebe foco de teclado na layer"] = click(
            state.get("launcher", {}).get("searchRect")
        ) and wait_for(
            lambda: re.search(r"TESTE foco de teclado alterado[^\n]*is_panel=true", clean()) is not None
        )
        type_text("ged")
        checks["digitar ged filtra o painel em tempo real"] = wait_for(
            lambda: (panel_state() or {}).get("launcher", {}).get("query") == "ged"
            and [app["id"] for app in (panel_state() or {})["launcher"]["apps"]] == ["aero-test-gedit"]
        )
        type_text("\b\b\bapp inexistente")
        checks["busca inexistente mostra lista vazia no painel"] = wait_for(
            lambda: (panel_state() or {}).get("launcher", {}).get("query") == "app inexistente"
            and (panel_state() or {})["launcher"]["apps"] == []
        )
        type_text("\b" * len("app inexistente") + "ged")
        wait_for(lambda: (panel_state() or {}).get("launcher", {}).get("query") == "ged")
        state = panel_state() or {}
        gedit_rect = state.get("launcher", {}).get("appRects", {}).get("aero-test-gedit")
        checks["clique no app do painel lança no workspace ativo"] = click(gedit_rect) and wait_for(
            lambda: len(workspaces().get(1, {}).get("windows", [])) == 1
            and workspaces()[1]["active"]
            and (panel_state() or {}).get("launcher", {}).get("open") is False
        )
        launched_ids = window_ids()
        state = panel_state() or {}
        click(state.get("launcher", {}).get("toggleRect"))
        wait_for(lambda: (panel_state() or {}).get("launcher", {}).get("open") is True)
        state = panel_state() or {}
        click(state.get("launcher", {}).get("appRects", {}).get("aero-test-gedit"))
        time.sleep(0.7)
        checks["clique repetido foca sem duplicar janela"] = window_ids() == launched_ids and len(launched_ids) == 1
        checks["zero erros de protocolo"] = "ProtocolError" not in clean()
    except (OSError, RuntimeError, IndexError, AttributeError, json.JSONDecodeError) as err:
        checks["execução do cenário sem exceção"] = False
        print(f"EXCEÇÃO: {err}")
    finally:
        print(f"LOGS: {OUT}")
        print("=== eventos (cru) ===")
        if events is not None:
            print(" ".join(events.kinds()))
            events.close()
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {label}")
        verdict = "OK" if checks and all(checks.values()) else "FALHA"
        print("VEREDITO: " + verdict)
        stop(shell)
        stop(comp)
    return 0 if verdict == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
