#!/usr/bin/env bash
# S3: input region derivada do DOM + ponteiro sintético no backend headless.
set -euo pipefail
cd "$(dirname "$0")/../.."

sock="${TMPDIR:-/tmp}/aero-s3-input-$$.sock"
socket_name="wayland-aero-s3-$$"
comp_log="${TMPDIR:-/tmp}/aero-s3-comp-$$.log"
shell_log="${TMPDIR:-/tmp}/aero-s3-shell-$$.log"
rm -f "$sock"

AERO_TEST_INPUT_SOCK="$sock" RUST_LOG=debug \
  target/debug/aeroshell-compositor --backend winit --socket "$socket_name" \
  >"$comp_log" 2>&1 &
comp=$!
trap 'kill "$comp" 2>/dev/null || true; rm -f "$sock"' EXIT

for _ in $(seq 1 50); do [ -S "$sock" ] && break; sleep 0.1; done

WAYLAND_DISPLAY="$socket_name" GDK_BACKEND=wayland AERO_SHELL_EXIT_AFTER=10 \
  LD_PRELOAD="${GTK4_LAYER_SHELL_LIB:-libgtk4-layer-shell.so.0}" RUST_LOG=debug \
  timeout 13s target/debug/aeroshell-shell >"$shell_log" 2>&1 || true

sleep 5
grep -q 'input region aplicada' "$shell_log" || {
  echo "FALHA: shell não aplicou input region" >&2
  cat "$shell_log" >&2
  exit 1
}
grep 'input region aplicada' "$shell_log" | tail -1

read -r gx gy rx ry rw rh <<<"$(python3 - "$shell_log" "$comp_log" <<'PY'
import re, sys
ansi = re.compile(r'\x1b\[[0-9;]*m')
shell = ansi.sub('', open(sys.argv[1], errors='ignore').read())
comp = ansi.sub('', open(sys.argv[2], errors='ignore').read())
m = re.findall(r'rects\s*=\s*\[?\(?(\d+),\s*(\d+),\s*(\d+),\s*(\d+)\)?', shell)
rx, ry, rw, rh = m[-1] if m else ('0','0','1','1')
g = re.findall(r'geometry\s*=\s*Some\(Rectangle<[^>]*>\s*\{\s*x:\s*(-?\d+),\s*y:\s*(-?\d+)', comp)
gx, gy = g[-1] if g else ('0','602')
print(gx, gy, rx, ry, rw, rh)
PY
)"
cx=$((gx + rx + rw / 2)); cy=$((gy + ry + rh / 2))
ox=$((gx + rx + rw + 200)); oy=$cy
echo "geometry=$gx,$gy rect_interativa=$rx,$ry ${rw}x${rh} dentro=$cx,$cy fora=$ox,$oy"

python3 - "$sock" "$cx" "$cy" "$ox" "$oy" <<'PY'
import socket, sys, time
path, cx, cy, ox, oy = sys.argv[1], *sys.argv[2:]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(path)
def send(line):
    s.sendall((line + "\n").encode())
    time.sleep(0.4)
send(f"motion {cx} {cy}")
send("button press")
send("button release")
send(f"motion {ox} {oy}")
send("button press")
send("button release")
s.close()
PY

sleep 0.5
kill "$comp" 2>/dev/null || true
wait "$comp" 2>/dev/null || true

echo "===== log do compositor (hit-test) ====="
grep -E 'TESTE ponteiro motion|TESTE ponteiro button' "$comp_log"

inside_line=$(grep -F "x=$cx y=$cy" "$comp_log" | head -1 || true)
outside_line=$(grep -F "x=$ox y=$oy" "$comp_log" | head -1 || true)

if [ -z "$inside_line" ] || [[ "$inside_line" != *"under = Some"* ]]; then
  echo "FALHA: clique dentro da área interativa não alcançou a layer surface" >&2
  exit 1
fi
if [ -z "$outside_line" ] || [[ "$outside_line" != *"under = None"* ]]; then
  echo "FALHA: clique fora da área interativa não passou através" >&2
  exit 1
fi
echo "OK S3: clique no verde alcança a superfície; clique transparente passa através"
