#!/usr/bin/env bash
# scripts/setup-deps.sh — dependências de sistema p/ compilar e testar o AeroShell (Ubuntu 26.04)
# Uso: bash scripts/setup-deps.sh   (ou chmod +x e ./scripts/setup-deps.sh)
set -euo pipefail

sudo apt update
sudo apt install -y \
  build-essential pkg-config cmake clang libclang-dev git curl \
  libwayland-dev wayland-protocols libxkbcommon-dev \
  libgtk-4-dev libwebkitgtk-6.0-dev libgtk4-layer-shell-dev libcairo2-dev \
  libegl-dev libgles-dev libgl-dev libegl1-mesa-dev libgles2-mesa-dev \
  libgl1-mesa-dri mesa-utils \
  libdrm-dev libgbm-dev libinput-dev libudev-dev libsystemd-dev \
  python3 foot wayland-utils dbus

# libseat-dev; fallback para libelogind-dev se o nome não existir na imagem
if ! sudo apt install -y libseat-dev; then
  sudo apt install -y libelogind-dev
fi

echo "OK: dependências de sistema instaladas."
echo "Faltam (fora do apt): rustup e bun. Depois rode: bash scripts/doctor.sh"
