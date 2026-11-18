#!/usr/bin/env bash
# S5 protocolo: filtro de globais + get_frame em janela inexistente (corrida).
set -euo pipefail
cd "$(dirname "$0")/../.."

export LD_LIBRARY_PATH=/tmp/opencode/aptroot/usr/lib/x86_64-linux-gnu
cargo build -p aeroshell-compositor -p aeroshell-frame-client >/dev/null
cargo test -p aeroshell-compositor protocols::aero_frame::tests >/dev/null

COMP=target/debug/aeroshell-compositor
CLIENT=target/debug/aeroshell-frame-client

# (a) cliente comum não vê o global: nenhum PID privilegiado configurado.
sock1="wayland-aero-frame-a-$$"
RUST_LOG=error "$COMP" --backend winit --socket "$sock1" >/tmp/aero-frame-a.log 2>&1 &
comp1=$!
trap 'kill "$comp1" 2>/dev/null || true' EXIT
sleep 1
WAYLAND_DISPLAY="$sock1" AERO_EXPECT_NO_GLOBAL=1 "$CLIENT" >/tmp/aero-frame-a-client.log 2>&1
grep -q 'não é visível' /tmp/aero-frame-a-client.log
kill "$comp1" 2>/dev/null || true
wait "$comp1" 2>/dev/null || true
echo "OK (a): cliente comum não vê aero_frame_manager_v1"

# (b) corrida: janela inexistente => objeto criado + closed, sem erro fatal.
sock2="wayland-aero-frame-b-$$"
WAYLAND_DISPLAY="$sock2" AERO_WAIT_SOCKET=1 AERO_FRAME_WINDOW_ID=999 \
  AERO_EXPECT_CLOSED=1 "$CLIENT" >/tmp/aero-frame-b-client.log 2>&1 &
client_pid=$!
sleep 0.2
AERO_SHELL_PID=$client_pid RUST_LOG=info "$COMP" --backend winit --socket "$sock2" \
  >/tmp/aero-frame-b.log 2>&1 &
comp2=$!
wait "$client_pid"
kill "$comp2" 2>/dev/null || true
wait "$comp2" 2>/dev/null || true
grep -q 'closed recebido' /tmp/aero-frame-b-client.log
! grep -q 'protocol error' /tmp/aero-frame-b.log
echo "OK (b): window_id inexistente gerou closed, não erro fatal"

echo "OK: unit tests de already_constructed e serial"
