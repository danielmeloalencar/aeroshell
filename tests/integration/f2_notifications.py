#!/usr/bin/env python3
"""F2-notifications: SDK Bun real, stream IPC, timeouts e clique no toast.

Baseline esperado antes do cartão: API/eventos/toasts/comandos não existem → FALHA.

Pré-requisitos (bench DEPOIS do check.sh):
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-shell
  bun build packages/panel/src/index.tsx --target=browser --outfile=packages/panel/dist/app.js

Uso: python3 tests/integration/f2_notifications.py
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
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-notifications-", dir="/tmp/opencode"))
IPC = OUT / "ipc.sock"
INPUT = OUT / "input.sock"
LOG = OUT / "compositor.log"
OUTPUT_HEIGHT = 600
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def wait_for(predicate, timeout=8):
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


def clean():
    return ANSI.sub("", LOG.read_text(errors="replace")) if LOG.exists() else ""


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

    def shown(self, notification_id=None):
        with self.lock:
            return [
                value["NotificationShown"]["notification"]
                for value in self.values
                if "NotificationShown" in value
                and (notification_id is None
                     or value["NotificationShown"]["notification"]["id"] == notification_id)
            ]

    def dismissed(self, notification_id):
        with self.lock:
            return any(
                value.get("NotificationDismissed", {}).get("id") == notification_id
                for value in self.values
            )

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


def panel_notifications():
    return (panel_state() or {}).get("notifications", [])


def notification_state(notification_id):
    return next((item for item in panel_notifications() if item.get("id") == notification_id), None)


def panel_geometry():
    # O log de geometria da LayerMap só é escrito no commit inicial. Durante o
    # toast a layer cresce ancorada na borda inferior: medimos a origem pelo
    # último `tamanho aceito` (altura) e pelo output 1000x600 do harness.
    heights = re.findall(r"tamanho aceito pela layer surface[^\n]*h:\s*(\d+)", clean())
    height = int(heights[-1]) if heights else 48
    return 0, OUTPUT_HEIGHT - height


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


def sdk_show(env, title, body, icon, timeout_ms):
    script = (
        'import { createAero, parseEvent } from "' + str(ROOT / "packages/sdk/src/index.ts") + '";\n'
        'import { ipcTransport, eventStream } from "' + str(ROOT / "packages/runtime/src/ipc-client.ts") + '";\n'
        'const path = process.env.AERO_WINDOW_IPC_SOCK;\n'
        'const aero = createAero(ipcTransport(path));\n'
        'const close = eventStream(path, raw => { const event = parseEvent(raw); if (event) aero.store.applyEvent(event); });\n'
        'let seen = null;\n'
        'aero.onNotification(n => { if (n.id === id) seen = n; });\n'
        f'const id = aero.showNotification({json.dumps(title)}, {json.dumps(body)}, {json.dumps(icon)}, {timeout_ms});\n'
        'const deadline = Date.now() + 5000;\n'
        'while (!seen && Date.now() < deadline) await new Promise(r => setTimeout(r, 25));\n'
        'if (!seen) throw new Error("onNotification não recebeu " + id);\n'
        'console.log("SDK_NOTIFICATION " + JSON.stringify({id, seen, active: aero.store.getNotifications()}));\n'
        'close();\n'
    )
    result = subprocess.run(
        ["bun", "-e", script], cwd=ROOT, env=env,
        capture_output=True, text=True, timeout=15,
    )
    print("=== SDK Bun real (cru) ===")
    print(result.stdout, end="")
    print(result.stderr, end="")
    match = re.search(r"SDK_NOTIFICATION (.+)", result.stdout)
    return (json.loads(match.group(1)) if match else None), result


def main():
    wayland = f"wayland-aero-f2-notifications-{os.getpid()}"
    env = {
        **os.environ,
        "RUST_LOG": "debug",
        "AERO_WINDOW_IPC_SOCK": str(IPC),
        "AERO_TEST_INPUT_SOCK": str(INPUT),
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
        shell = subprocess.Popen(
            ["dbus-run-session", "--", str(ROOT / "target/debug/aeroshell-shell")],
            cwd=ROOT,
            env={**env, "WAYLAND_DISPLAY": wayland, "GDK_BACKEND": "wayland",
                 "GTK4_LAYER_SHELL_LIB": os.environ.get("GTK4_LAYER_SHELL_LIB", "libgtk4-layer-shell.so.0")},
            stdout=handle, stderr=subprocess.STDOUT, start_new_session=True,
        )
        checks["painel React pronto"] = wait_for(lambda: panel_state() is not None, 12)

        first, first_run = sdk_show(env, "Teste", "Corpo da mensagem", "utilities-terminal", 5000)
        checks["SDK showNotification/onNotification/estado ativo"] = bool(first)
        first_id = first["id"] if first else "ausente"
        checks["toast padrão tem título, corpo e ícone"] = bool(first) and wait_for(
            lambda: (notification_state(first_id) or {}).get("title") == "Teste"
            and (notification_state(first_id) or {}).get("body") == "Corpo da mensagem"
            and (notification_state(first_id) or {}).get("iconSource") == "utilities-terminal"
        )
        checks["NotificationShown chegou ao stream"] = bool(first) and wait_for(lambda: bool(events.shown(first_id)))
        checks["toast padrão desaparece após 5 s"] = bool(first) and wait_for(
            lambda: notification_state(first_id) is None and events.dismissed(first_id), 8
        )

        second, second_run = sdk_show(env, "Longa", "Persiste por dez segundos", None, 10000)
        checks["segunda API pública também funciona"] = bool(second)
        second_id = second["id"] if second else "ausente"
        checks["ícone padrão é usado quando icon é omitido"] = bool(second) and wait_for(
            lambda: (notification_state(second_id) or {}).get("iconSource") == "dialog-information"
        )
        if second:
            time.sleep(5.5)
        checks["timeout de 10000 ms persiste após 5 s"] = bool(second) and notification_state(second_id) is not None
        close_rect = (panel_state() or {}).get("notificationRects", {}).get(second_id)
        checks["clique em X fecha imediatamente"] = bool(second) and click(close_rect) and wait_for(
            lambda: notification_state(second_id) is None and events.dismissed(second_id), 2
        )

        raw_notify = command("notify IPC Cliente desconectado")
        print(f"=== notify IPC (cru): {raw_notify} ===")
        checks["comando notify aceita título e corpo com espaços"] = raw_notify == ["OK"] and wait_for(
            lambda: any(n["title"] == "IPC" and n["body"] == "Cliente desconectado" for n in events.shown())
        )
        ipc_notification = next(
            (n for n in events.shown() if n["title"] == "IPC" and n["body"] == "Cliente desconectado"), None
        )
        checks["notify do compositor vira toast"] = bool(ipc_notification) and wait_for(
            lambda: notification_state(ipc_notification["id"]) is not None
        )
        if ipc_notification:
            raw_dismiss = command(f"dismiss-notification {json.dumps(ipc_notification['id'])}")
            checks["dismissNotification/IPC publica remoção"] = raw_dismiss == ["OK"] and wait_for(
                lambda: notification_state(ipc_notification["id"]) is None
                and events.dismissed(ipc_notification["id"]), 2
            )
        else:
            checks["dismissNotification/IPC publica remoção"] = False
        checks["zero erros de protocolo"] = "ProtocolError" not in clean()
    except (OSError, RuntimeError, IndexError, AttributeError, json.JSONDecodeError, subprocess.TimeoutExpired) as err:
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
