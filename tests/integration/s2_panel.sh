#!/usr/bin/env bash
# S2: painel GTK4/WebKitGTK como layer surface, exclusive zone e WebView transparente.
# Requer libgtk4-layer-shell-dev instalado. Se GTK4_LAYER_SHELL_LIB for informado,
# o caminho é usado como LD_PRELOAD (build local sem installer no sistema).
set -euo pipefail

cd "$(dirname "$0")/../.."
cargo build -p aeroshell-compositor -p aeroshell-shell >/dev/null

socket="wayland-aero-s2-$$"
comp_log="${TMPDIR:-/tmp}/aeroshell-s2-comp.log"
shell_log="${TMPDIR:-/tmp}/aeroshell-s2-shell.log"

RUST_LOG=info target/debug/aeroshell-compositor --backend winit --socket "$socket" \
  >"$comp_log" 2>&1 &
comp=$!
trap 'kill "$comp" 2>/dev/null || true' EXIT

for _ in $(seq 1 50); do
  [ -S "${XDG_RUNTIME_DIR:-/tmp}/$socket" ] && break
  sleep 0.1
done

preload_env=()
if [ -n "${GTK4_LAYER_SHELL_LIB:-}" ]; then
  preload_env=(env LD_PRELOAD="$GTK4_LAYER_SHELL_LIB")
fi

WAYLAND_DISPLAY="$socket" GDK_BACKEND=wayland RUST_LOG=info \
  timeout 6s "${preload_env[@]}" target/debug/aeroshell-shell >"$shell_log" 2>&1 || true

kill "$comp" 2>/dev/null || true
sleep 0.3

grep -q "nova layer surface" "$comp_log" || { echo "FALHA: layer surface não criada" >&2; cat "$comp_log" >&2; exit 1; }
grep -q "Exclusive(48)" "$comp_log" || { echo "FALHA: exclusive zone 48 não recebida" >&2; exit 1; }
grep -q "WebView com fundo transparente" "$shell_log" || { echo "FALHA: WebView não está transparente" >&2; cat "$shell_log" >&2; exit 1; }

echo "OK: painel layer-shell com exclusive zone 48 e WebView transparente"
