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
