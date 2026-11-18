set shell := ["bash", "-uc"]

bootstrap:
    sudo apt-get update
    sudo apt-get install -y build-essential pkg-config cmake git curl just \
      libgtk-4-dev libwebkitgtk-6.0-dev libgtk4-layer-shell-dev \
      libwayland-dev wayland-protocols libxkbcommon-dev \
      libdrm-dev libgbm-dev libudev-dev libinput-dev libseat-dev \
      libpam0g-dev libdbus-1-dev dbus

check:
    bash scripts/doctor.sh
    cargo check --workspace --all-targets
    bun test

fmt:
    cargo fmt --all

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

types:
    cargo test -p aeroshell-ipc --features export-bindings

dev *args:
    bun packages/cli/src/index.ts dev {{args}}

nested:
    cargo run -p aeroshell-compositor -- --backend winit --socket wayland-aero-0

bench:
    @echo "AVISO: benchmarks de GPU só em hardware real; VM = funcionamento"

spike s:
    @echo "Spike {{s}}: ver docs/progress.md"
