#!/usr/bin/env python3
"""F2-workspaces: comandos, eventos e visibilidade real na cena.

Baseline esperado antes do cartão: `workspace-create` é desconhecido, não há
eventos nem sondas WORKSPACE_RENDER → VEREDITO: FALHA.

Pré-requisitos:
  cargo build -p aeroshell-compositor --features bench
  cargo build -p aeroshell-shell --example frame_webview

Uso: python3 tests/integration/f2_workspaces.py
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
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-workspaces-", dir="/tmp/opencode"))
IPC = OUT / "ipc.sock"


def wait_for(predicate, timeout=12):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
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


class Events:
    def __init__(self, path):
        self.values = []
        self.lock = threading.Lock()
        self.ready = threading.Event()
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(str(path))
        self.thread = threading.Thread(target=self._read, daemon=True)
        self.thread.start()

    def _read(self):
        self.ready.set()
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

    def kinds(self):
        with self.lock:
            return [next(iter(value), "") for value in self.values]

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


def launch_foot(wayland, title):
    env = {**os.environ, "WAYLAND_DISPLAY": wayland, "GDK_BACKEND": "wayland"}
    env.pop("DISPLAY", None)
    return subprocess.Popen(
        ["foot", "-T", title], cwd=ROOT, env=env,
        stdout=subprocess.DEVNULL, stderr=(OUT / f"{title}.err").open("wb"),
        start_new_session=True,
    )


def workspaces():
    result = {}
    for line in command("workspace-list", multiline=True):
        match = re.match(r"WS (\d+) active=([01]) windows=([0-9,]*) (.*)$", line)
        if match:
            result[int(match.group(1))] = {
                "active": match.group(2) == "1",
                "windows": [int(v) for v in match.group(3).split(",") if v],
                "name": json.loads(match.group(4)),
            }
    return result


def latest_visibility(log, window_id):
    values = re.findall(
        rf"WORKSPACE_RENDER[^\n]*window_id={window_id}[^\n]*visible=(true|false)", log
    )
    frames = re.findall(
        rf"WORKSPACE_FRAME_RENDER[^\n]*window_id={window_id}[^\n]*visible=(true|false)", log
    )
    return (values[-1] == "true" if values else None,
            frames[-1] == "true" if frames else None)


def main():
    wayland = f"wayland-aero-f2-workspaces-{os.getpid()}"
    log_path = OUT / "compositor.log"
    log_handle = log_path.open("wb")
    comp = None
    feet = []
    events = None
    checks = {}
    try:
        env = {**os.environ, "RUST_LOG": "debug", "AERO_WINDOW_IPC_SOCK": str(IPC)}
        for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
            env.pop(key, None)
        comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
             "--socket", wayland, "--size", "1000x600",
             "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, env=env, stdout=log_handle, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        ready = wait_for(lambda: (RUNTIME / wayland).exists() and IPC.exists(), 15)
        event_ready = wait_for(lambda: pathlib.Path(f"{IPC}.events").exists(), 15)
        checks["compositor e IPC prontos"] = ready and event_ready
        if not checks["compositor e IPC prontos"]:
            raise RuntimeError("compositor/IPC não ficou pronto")
        events = Events(pathlib.Path(f"{IPC}.events"))
        checks["snapshot inicial tem workspace 1 ativa"] = wait_for(
            lambda: workspaces().get(1, {}).get("active") is True
        )

        feet.append(launch_foot(wayland, "workspace-A"))
        checks["foot A mapeou no workspace inicial"] = wait_for(
            lambda: workspaces().get(1, {}).get("windows") == [0]
        )
        created = command("workspace-create Trabalho")[0]
        match = re.match(r"WORKSPACE (\d+)", created)
        second = int(match.group(1)) if match else 2
        checks["criar workspace devolve e lista o novo"] = wait_for(
            lambda: second in workspaces() and workspaces()[second]["name"] == "Trabalho"
        )
        command(f"workspace-rename {second} Pesquisa")
        checks["renomear workspace"] = wait_for(
            lambda: workspaces().get(second, {}).get("name") == "Pesquisa"
        )
        checks["evento WorkspaceCreated"] = events.has("WorkspaceCreated")
        checks["evento WorkspaceRenamed"] = events.has("WorkspaceRenamed", id=second, name="Pesquisa")

        command(f"workspace-activate {second}")
        checks["ativar workspace 2"] = wait_for(
            lambda: workspaces().get(second, {}).get("active") is True
        )
        feet.append(launch_foot(wayland, "workspace-B"))
        checks["nova janela entra no workspace ativo"] = wait_for(
            lambda: workspaces().get(second, {}).get("windows") == [1]
        )
        command("workspace-activate 1")
        checks["eventos WorkspaceActivated"] = (
            events.has("WorkspaceActivated", id=second) and events.has("WorkspaceActivated", id=1)
        )
        checks["workspace 1 contém só A; workspace 2 contém só B"] = wait_for(
            lambda: workspaces().get(1, {}).get("windows") == [0]
            and workspaces().get(second, {}).get("windows") == [1]
        )
        time.sleep(1)
        visibility = latest_visibility(log_path.read_text(errors="replace"), 0)
        visibility_b = latest_visibility(log_path.read_text(errors="replace"), 1)
        checks["ativar mostra A e esconde B na cena e nos frames"] = (
            visibility == (True, True) and visibility_b == (False, False)
        )

        moved = command(f"window-workspace 0 {second}")[0]
        checks["mover A para workspace 2"] = moved == "OK" and wait_for(
            lambda: workspaces().get(1, {}).get("windows") == []
            and workspaces().get(second, {}).get("windows") == [0, 1]
        )
        checks["evento WindowMovedToWorkspace"] = events.has(
            "WindowMovedToWorkspace", window_id=0, workspace_id=second
        )
        time.sleep(1)
        checks["mover A para inativa esconde cliente e frame"] = (
            latest_visibility(log_path.read_text(errors="replace"), 0) == (False, False)
        )
        command(f"workspace-activate {second}")
        time.sleep(1)
        checks["ativar destino mostra A e B sem recriar clientes"] = (
            latest_visibility(log_path.read_text(errors="replace"), 0) == (True, True)
            and latest_visibility(log_path.read_text(errors="replace"), 1) == (True, True)
            and all(proc.poll() is None for proc in feet)
        )

        nonempty = command(f"workspace-destroy {second}")[0]
        checks["destruir com janelas é recusado"] = nonempty.startswith("ERR workspace não vazio")

        # Fechar um cliente real deve remover o ID de QUALQUER workspace;
        # ocultar por troca de workspace, por outro lado, não pode destruí-lo.
        third_response = command("workspace-create Limpeza")[0]
        third = int(re.match(r"WORKSPACE (\d+)", third_response).group(1))
        command(f"workspace-activate {third}")
        command(f"window-workspace 1 {third}")
        checks["mover B para workspace temporário"] = wait_for(
            lambda: workspaces().get(third, {}).get("windows") == [1]
        )
        stop(feet.pop(1))
        checks["fechar cliente limpa membership"] = wait_for(
            lambda: workspaces().get(third, {}).get("windows") == []
        )
        command(f"workspace-destroy {third}")
        command("window-workspace 0 1")
        command("workspace-activate 1")
        destroyed = command(f"workspace-destroy {second}")[0]
        checks["destruir vazio remove da lista"] = destroyed == "OK" and wait_for(
            lambda: second not in workspaces()
        )
        checks["evento WorkspaceDestroyed"] = events.has("WorkspaceDestroyed", id=second)
        checks["último workspace é recusado"] = command("workspace-destroy 1")[0].startswith("ERR último workspace")

        sdk_script = (
            'import { createAero } from "' + str(ROOT / "packages/sdk/src/index.ts") + '";\n'
            'import { ipcTransport } from "' + str(ROOT / "packages/runtime/src/ipc-client.ts") + '";\n'
            'const aero = createAero(ipcTransport(process.env.AERO_WINDOW_IPC_SOCK));\n'
            'const before = await aero.listWorkspaces();\n'
            'const created = await aero.createWorkspace("SDK Bun real");\n'
            'await aero.activateWorkspace(created.id);\n'
            'await aero.activateWorkspace(1);\n'
            'await aero.destroyWorkspace(created.id);\n'
            'const after = await aero.listWorkspaces();\n'
            'if (before.length !== 1 || after.length !== 1) throw new Error("lista inconsistente");\n'
            'console.log("SDK_WORKSPACES OK");\n'
        )
        sdk_run = subprocess.run(
            ["bun", "-e", sdk_script], cwd=ROOT, env=env,
            capture_output=True, text=True, timeout=20,
        )
        print("=== SDK Bun real (cru) ===")
        print(sdk_run.stdout, end="")
        print(sdk_run.stderr, end="")
        checks["SDK Bun usa o transporte IPC real"] = "SDK_WORKSPACES OK" in sdk_run.stdout

        checks["clientes continuam vivos após alternâncias"] = all(proc.poll() is None for proc in feet)
        checks["zero erros de protocolo"] = "ProtocolError" not in log_path.read_text(errors="replace")
    except (OSError, RuntimeError, IndexError, json.JSONDecodeError) as err:
        checks["execução do cenário sem exceção"] = False
        print(f"EXCEÇÃO: {err}")
    finally:
        print(f"LOGS: {OUT}")
        print("=== workspaces ao final (cru) ===")
        try:
            print("\n".join(command("workspace-list", multiline=True)))
        except OSError as err:
            print(f"IPC indisponível: {err}")
        if events is not None:
            print("=== eventos (cru) ===")
            print(" ".join(events.kinds()))
            events.close()
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {label}")
        verdict = "OK" if checks and all(checks.values()) else "FALHA"
        print("VEREDITO: " + verdict)
        for proc in feet:
            stop(proc)
        stop(comp)
        log_handle.close()
    return 0 if verdict == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
