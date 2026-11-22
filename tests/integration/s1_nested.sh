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
# EXIT cobre a saída normal; TERM/INT cobre o kill vindo do teste que iniciou
# este script. Sem eles, o finally do teste mataria o bash e deixaria o
# compositor filho órfão (critério 2 da tarefa 00).
cleanup() { kill "$pid" 2>/dev/null || true; }
trap cleanup EXIT
trap 'cleanup; exit 143' TERM INT

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
  # Tarefa 00 (escopo infra): forcar o cliente GUI ao socket nested de forma
  # explicita no harness. O supervisor ja aplica esse isolamento em
  # spawn_public_client (supervisor.rs:141-150), mas o caminho do bun test ja
  # falhou uma vez com `zenity: Gtk-WARNING: Failed to open display` — repetir
  # aqui torna o S1 independente do env do chamador (STATE.md §6).
  RUST_LOG=info "$bin" --backend winit --socket "$nested_socket" \
    --command "env -u DISPLAY -u WAYLAND_SOCKET GDK_BACKEND=wayland WAYLAND_DISPLAY=$nested_socket zenity --info --text=AeroShell-S1" \
    >"$nested_log" 2>&1 &
  pid=$!
  cleanup() { kill "$pid" 2>/dev/null || true; }
  trap cleanup EXIT
  trap 'cleanup; exit 143' TERM INT
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
