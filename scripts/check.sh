#!/usr/bin/env bash
# Verificação única do projeto: substitui `just check` (just não está instalado).
# CI, agentes e desenvolvimento usam exatamente este script.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== doctor =="
bash scripts/doctor.sh

echo "== fmt =="
cargo fmt --all -- --check

echo "== clippy =="
cargo clippy --workspace --all-targets -- -D warnings

echo "== cargo test =="
cargo test --workspace

echo "== bun test =="
bun test

echo "OK: scripts/check.sh passou"
