#!/usr/bin/env bash
# S1: sobe o compositor em modo headless, valida socket próprio e isolamento do host.
set -euo pipefail

cd "$(dirname "$0")/../.."
cargo build -p aeroshell-compositor >/dev/null

host_display="${WAYLAND_DISPLAY:-}"
socket="wayland-aero-s1-$$"
bin="target/debug/aeroshell-compositor"

"$bin" --backend headless --socket "$socket" >/tmp/aeroshell-s1.log 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || true' EXIT

for _ in $(seq 1 50); do
  [ -S "${XDG_RUNTIME_DIR:-/tmp}/$socket" ] && break
  sleep 0.1
done

socket_path="${XDG_RUNTIME_DIR:-/tmp}/$socket"
if [ ! -S "$socket_path" ]; then
  echo "FALHA: socket $socket_path não criado" >&2
  cat /tmp/aeroshell-s1.log >&2
  exit 1
fi

if [ "${WAYLAND_DISPLAY:-}" != "$host_display" ]; then
  echo "FALHA: WAYLAND_DISPLAY do host foi alterado" >&2
  exit 1
fi

echo "OK: socket próprio $socket criado; WAYLAND_DISPLAY do host intacto ($host_display)"
kill "$pid" 2>/dev/null || true
wait "$pid" 2>/dev/null || true

# Uma segunda fase valida que um cliente GTK realmente mapeia um toplevel DENTRO
# do compositor. Sem remover DISPLAY, o GTK3 vai para o X11 do host e nada aparece.
if command -v zenity >/dev/null 2>&1; then
  nested_socket="wayland-aero-s1-visual-$$"
  nested_log="${TMPDIR:-/tmp}/aeroshell-s1-visual.log"
  RUST_LOG=info "$bin" --backend winit --socket "$nested_socket" \
    --command "zenity --info --text=AeroShell-S1" >"$nested_log" 2>&1 &
  pid=$!
  trap 'kill "$pid" 2>/dev/null || true' EXIT
  sleep 4
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  grep -q "novo toplevel xdg-surface mapeado" "$nested_log" || {
    echo "FALHA: cliente não mapeou toplevel dentro do compositor" >&2
    cat "$nested_log" >&2
    exit 1
  }
  echo "OK: cliente GTK3 mapeou toplevel dentro da janela do AeroShell"
fi
