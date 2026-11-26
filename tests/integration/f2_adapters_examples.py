#!/usr/bin/env python3
"""F2-adapters+exemplos: React/Vue/Solid/Three, produção, HMR, animação e alfa.

Baseline esperado: adapters/exemplos ausentes -> VEREDITO: FALHA no preflight.
Pré-requisitos (bench POR ÚLTIMO):
  cargo build --workspace --examples
  cargo build -p aeroshell-shell
  cargo build -p aeroshell-compositor --features bench
"""
import hashlib
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
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path(tempfile.mkdtemp(prefix="aero-f2-adapters-", dir="/tmp/opencode"))
RUNTIME_DIR = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"))
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def wait_for(predicate, timeout=25):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if predicate():
                return True
        except (OSError, ValueError, KeyError, IndexError, json.JSONDecodeError):
            pass
        time.sleep(0.1)
    return False


def clean(path):
    return ANSI.sub("", path.read_text(errors="replace")) if path.exists() else ""


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


def command(ipc, line, dot=False):
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
            if not dot and raw.startswith(("OK", "ERR")):
                break
        return lines


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def damage_area(value):
    if value in ("none", "submitted", "", None):
        return 0
    total = 0
    for rect in re.split(r"[;,]", value or ""):
        match = re.fullmatch(r"(\d+)x(\d+)@-?\d+,-?\d+", rect)
        if match:
            total += int(match.group(1)) * int(match.group(2))
    return total


def headless_damage(text):
    rows = re.findall(r"HEADLESS_FRAME output=\S+ age=(\d+) damage=(none|[0-9x@,;-]+)", text)
    return sum(damage_area(damage) for _, damage in rows), len(rows)


def preflight():
    required = [
        "examples/blank-desktop/panel.ts",
        "examples/win11-like/desktop.config.tsx",
        "examples/win11-like/panel.ts",
        "examples/win11-like/start-menu.ts",
        "examples/threejs-desktop/background.js",
        "examples/threejs-desktop/widgets.tsx",
    ]
    missing = [name for name in required if not (ROOT / name).exists()]
    source = """
      import * as react from './packages/react/src/index.ts';
      import * as solid from './packages/solid/src/index.ts';
      import * as vue from './packages/vue/src/index.ts';
      for (const adapter of [react, solid, vue]) {
        for (const name of ['AeroProvider', 'mountSurface', 'useWindows', 'useWorkspaces',
          'useOutputs', 'useApps', 'useEvent', 'useSurface']) {
          if (!adapter[name]) throw new Error('export ausente: ' + name);
        }
      }
    """
    result = subprocess.run(["bun", "-e", source], cwd=ROOT, capture_output=True, text=True)
    print("=== preflight adapters/exemplos (cru) ===")
    print("missing=" + json.dumps(missing))
    print(result.stdout, end="")
    print(result.stderr, end="")
    return not missing and result.returncode == 0


def fixture(example):
    source = ROOT / "examples" / example
    target = OUT / example
    if target.exists():
        return target
    shutil.copytree(source, target, ignore=shutil.ignore_patterns("node_modules"))
    if (source / "node_modules").exists():
        (target / "node_modules").symlink_to(source / "node_modules", target_is_directory=True)
    for path in target.iterdir():
        if path.suffix not in (".ts", ".tsx", ".js", ".json"):
            continue
        text = path.read_text()
        text = text.replace('"../../packages/', json.dumps(str(ROOT / "packages"))[:-1] + "/")
        path.write_text(text)
    if example == "blank-desktop":
        config = target / "desktop.config.tsx"
        config.write_text(re.sub(
            r'\n    \{\n      kind: "Panel",\n      id: "blank-panel-output-1",[\s\S]*?\n    \},',
            "", config.read_text(),
        ))
    return target


class Scenario:
    def __init__(self, name, example, dev=False, copy=True):
        self.name = name
        self.dir = fixture(example) if copy else ROOT / "examples" / example
        self.config = self.dir / "desktop.config.tsx"
        self.dev = dev
        self.socket = f"wayland-aero-f2-adapters-{name}-{os.getpid()}"
        self.ipc = OUT / f"{name}.sock"
        self.input = OUT / f"{name}.input"
        self.log = OUT / f"{name}-compositor.log"
        self.host_log = OUT / f"{name}-host.log"
        self.runtime_log = OUT / f"{name}-runtime.log"
        self.pixels = OUT / f"{name}-pixels"
        self.pixels.mkdir()
        self.origin = f"http://127.0.0.1:{free_port()}" if dev else None
        self.comp = self.host = self.runtime = None
        self.input_sock = None

    def text(self):
        return clean(self.log) + "\n" + clean(self.host_log)

    def start(self):
        env = {
            **os.environ,
            "RUST_LOG": "info",
            "AERO_WINDOW_IPC_SOCK": str(self.ipc),
            "AERO_TEST_INPUT_SOCK": str(self.input),
            "AERO_PIXEL_DUMP_DIR": str(self.pixels),
            "AERO_PIXEL_FULL": "1",
            "AERO_SURFACE_HOST": "1",
        }
        if self.origin:
            env["AERO_DEV_ORIGIN"] = self.origin
        for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AERO_PRIVILEGED_FD"):
            env.pop(key, None)
        self.comp = subprocess.Popen(
            [str(ROOT / "target/debug/aeroshell-compositor"), "--backend", "headless",
             "--socket", self.socket, "--size", "640x480", "--command", "foot -w 300x220",
             "--shell", str(ROOT / "target/debug/examples/frame_webview"),
             "--shell-env", "LD_PRELOAD=libgtk4-layer-shell.so.0"],
            cwd=ROOT, env=env, stdout=self.log.open("wb"), stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        if not wait_for(lambda: self.ipc.exists() and self.input.exists() and (RUNTIME_DIR / self.socket).exists()):
            raise RuntimeError(f"compositor {self.name} não ficou pronto")
        self.host = subprocess.Popen(
            ["dbus-run-session", "--", str(ROOT / "target/debug/aeroshell-shell")],
            cwd=ROOT, env={**env, "WAYLAND_DISPLAY": self.socket, "GDK_BACKEND": "wayland",
                           "GTK4_LAYER_SHELL_LIB": "libgtk4-layer-shell.so.0"},
            stdout=self.host_log.open("wb"), stderr=subprocess.STDOUT, start_new_session=True,
        )
        if not wait_for(lambda: "SURFACE_HOST pronto" in clean(self.host_log)):
            raise RuntimeError(f"host {self.name} não ficou pronto")
        if not wait_for(lambda: any(line.startswith("W 0 ") for line in command(self.ipc, "list", dot=True))):
            raise RuntimeError(f"foot {self.name} não mapeou")
        command(self.ipc, "resize 0 260 200")
        command(self.ipc, "move 0 10 20")
        args = ["bun", "packages/runtime/src/main.ts", "--config", str(self.config)]
        if self.dev:
            args.append("--dev")
        self.runtime = subprocess.Popen(
            args, cwd=ROOT, env=env, stdout=self.runtime_log.open("wb"),
            stderr=subprocess.STDOUT, start_new_session=True,
        )
        self.input_sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.input_sock.connect(str(self.input))
        return env

    def send(self, line):
        self.input_sock.sendall((line + "\n").encode())

    def key(self, number):
        self.send("key press 125")
        self.send(f"key press {number}")
        self.send(f"key release {number}")
        self.send("key release 125")

    def probe(self, x, y):
        before = len(clean(self.log).splitlines())
        self.send(f"motion {x} {y}")
        result = []

        def collect():
            result.clear()
            for line in clean(self.log).splitlines()[before:]:
                if "TESTE ponteiro motion" not in line or "under=" not in line:
                    continue
                match = re.search(
                    r"under=Some\(\(ObjectId\(wl_surface@\d+\[\d+\], (\d+)\), "
                    r"Point<[^>]+> \{ x: (-?[\d.]+), y: (-?[\d.]+) \}\)", line
                )
                if match:
                    result.append(match.group(1))
                    break
            return bool(result)

        if not wait_for(collect, 6):
            return None
        return result[0]

    def pixel_snapshot(self, name):
        source = self.pixels / "full_000000.bgra"
        expected = 640 * 480 * 4
        for _ in range(30):
            if source.exists() and source.stat().st_size == expected:
                data = source.read_bytes()
                if len(data) == expected:
                    target = self.pixels / name
                    target.write_bytes(data)
                    return data
            time.sleep(0.05)
        return None

    def close(self):
        if self.input_sock:
            self.input_sock.close()
            self.input_sock = None
        for proc in (self.runtime, self.host, self.comp):
            stop(proc)


def guest_field(text, name, value):
    return f"{json.dumps(name)}:{json.dumps(value)}" in text


def changed_pixels(before, after):
    if before is None or after is None or len(before) != len(after):
        return 0
    changed = 0
    # ROI central: exclui painel/cursor e exige mudança RGB real, não só hash.
    for index in range((80 * 640 + 80) * 4, 400 * 640 * 4, 4):
        if index + 3 >= len(before):
            break
        if max(abs(before[index + channel] - after[index + channel]) for channel in range(3)) > 8:
            changed += 1
    return changed


def main():
    if not preflight():
        print(f"LOGS: {OUT}")
        print("VEREDITO: FALHA")
        return 1
    checks = {}
    scenarios = []
    try:
        blank = Scenario("blank", "blank-desktop", dev=False)
        scenarios.append(blank)
        blank.start()
        checks["blank React em produção aero://"] = wait_for(
            lambda: guest_field(blank.text(), "framework", "react")
        )
        checks["blank recebe janela inicial pelo replay"] = wait_for(
            lambda: guest_field(blank.text(), "windowCount", 1)
        )
        checks["hidratção usa replay existente"] = wait_for(
            lambda: "SURFACE_HYDRATE id=blank-panel" in clean(blank.runtime_log)
        )
        blank.close()
        scenarios.remove(blank)

        hmr = Scenario("blank-hmr", "blank-desktop", dev=True)
        scenarios.append(hmr)
        hmr.start()
        checks["blank HMR inicia React"] = wait_for(lambda: "MODULO_ANTES" in hmr.text())
        hmr_pids = (hmr.comp.pid, hmr.runtime.pid)
        panel = hmr.dir / "panel.ts"
        panel.write_text(panel.read_text().replace("MODULO_ANTES", "MODULO_DEPOIS"))
        checks["HMR módulo React sem restart"] = wait_for(lambda: "MODULO_DEPOIS" in hmr.text())
        checks["PIDs HMR preservados"] = (hmr.comp.pid, hmr.runtime.pid) == hmr_pids
        checks["HMR reidrata janela"] = wait_for(
            lambda: clean(hmr.runtime_log).count("SURFACE_HYDRATE id=blank-panel") >= 2
        )
        hmr.close()
        scenarios.remove(hmr)

        win = Scenario("win11", "win11-like", dev=True)
        scenarios.append(win)
        win.start()
        checks["win11 Vue/taskbar/apps reativos"] = wait_for(
            lambda: guest_field(win.text(), "framework", "vue")
        ) and wait_for(lambda: guest_field(win.text(), "windowCount", 1))
        checks["Overlay do Start pronta"] = wait_for(
            lambda: guest_field(win.text(), "id", "win11-start")
            and guest_field(win.text(), "menu", "closed")
        )
        win.key(4)  # evdev 4 = tecla 3
        checks["Super+3 abre Start por useEvent"] = wait_for(
            lambda: guest_field(win.text(), "menu", "open")
        )

        def first_app_rect():
            for line in reversed(clean(win.host_log).splitlines()):
                if "input region aplicada" not in line:
                    continue
                rects = re.findall(r"\((\d+), (\d+), (\d+), (\d+)\)", line)
                if len(rects) > 2:
                    return tuple(int(value) for value in rects[2])
            return None

        checks["Start publica rects dos apps"] = wait_for(lambda: first_app_rect() is not None)
        time.sleep(0.5)
        rect = first_app_rect()
        before = [line for line in command(win.ipc, "list", dot=True) if line.startswith("W ")]
        if rect:
            x, y, width, height = rect
            for line in (f"motion {x + width // 2} {y + height // 2}", "button press", "button release"):
                win.send(line)
                time.sleep(0.1)
        checks["clique no app do Start lança pelo IPC"] = wait_for(
            lambda: len([line for line in command(win.ipc, "list", dot=True) if line.startswith("W ")]) > len(before)
        )
        win.close()
        scenarios.remove(win)

        three = Scenario("three", "threejs-desktop", dev=True)
        scenarios.append(three)
        three.start()
        checks["Three WebGL e widget Solid prontos"] = wait_for(
            lambda: guest_field(three.text(), "framework", "three")
            and guest_field(three.text(), "framework", "solid"), 40
        )
        first = three.pixel_snapshot("animated_t0.bgra")
        time.sleep(0.5)
        second = three.pixel_snapshot("animated_t1.bgra")
        changed = changed_pixels(first, second)
        hashes = [hashlib.sha256(data or b"").hexdigest() for data in (first, second)]
        print(f"=== Three pixels: changed={changed} hashes={hashes} ===")
        checks["Three tem pixels realmente animados"] = changed >= 1000 and hashes[0] != hashes[1]
        foot_surface = three.probe(210, 190)
        widget_surface = three.probe(540, 36)
        print(f"=== alfa: foot={foot_surface} widget={widget_surface} ===")
        checks["widget opaco e área transparente têm hits distintos"] = bool(
            foot_surface and widget_surface and foot_surface != widget_surface
        )
        three.key(5)  # evdev 5 = tecla 4
        checks["Super+4 para a animação"] = wait_for(
            lambda: guest_field(three.text(), "running", "stopped")
        )
        time.sleep(1.0)
        mark = len(clean(three.log))
        time.sleep(2.0)
        damage, frames = headless_damage(clean(three.log)[mark:])
        print(f"=== Three parado: damage={damage} headless_frames={frames} ===")
        checks["cena estática não gera damage"] = damage == 0
        stopped0 = three.pixel_snapshot("stopped_t0.bgra")
        time.sleep(0.3)
        stopped1 = three.pixel_snapshot("stopped_t1.bgra")
        checks["pixels estáticos após parar"] = stopped0 is not None and stopped0 == stopped1
        checks["zero erros de protocolo nos exemplos"] = "ProtocolError" not in three.text()
        three.close()
        scenarios.remove(three)
    except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired) as err:
        checks["execução sem exceção"] = False
        print(f"EXCEÇÃO: {err}")
    finally:
        print(f"LOGS: {OUT}")
        for scenario in scenarios:
            print(f"=== {scenario.name} runtime (cru) ===")
            print(clean(scenario.runtime_log)[-3500:])
            print(f"=== {scenario.name} host (recorte cru) ===")
            for line in scenario.text().splitlines():
                if any(token in line for token in ("guest", "SURFACE_HOST", "ERROR", "Error")):
                    print(line)
            scenario.close()
        for label, ok in checks.items():
            print(f"{'OK ' if ok else 'FALHA '} {label}")
        verdict = "OK" if checks and all(checks.values()) else "FALHA"
        print("VEREDITO: " + verdict)
    return 0 if verdict == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
