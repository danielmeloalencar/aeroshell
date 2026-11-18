#!/usr/bin/env bash
# S5 protocolo: filtro por socketpair, role/already_constructed e validação de serial.
set -euo pipefail
cd "$(dirname "$0")/../.."

export LD_LIBRARY_PATH=/tmp/opencode/aptroot/usr/lib/x86_64-linux-gnu
cargo build -p aeroshell-compositor -p aeroshell-frame-client >/dev/null
cargo test -p aeroshell-compositor protocols::aero_frame::tests >/dev/null
python3 tests/integration/aero_frame_protocol.py
