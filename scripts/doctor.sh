#!/usr/bin/env bash
# Verifica dependências de build e imprime QUAL PACOTE APT falta, em vez de
# despejar um erro opaco do pkg-config. Não instala nada.
set -uo pipefail

cd "$(dirname "$0")/.."

missing=()
optional=()

check_pc() {
  local pc="$1" pkg="$2" version="${3:-}"
  if ! pkg-config --exists "$pc"; then
    missing+=("$pkg (pkg-config: $pc)")
    return
  fi
  if [ -n "$version" ]; then
    local actual
    actual=$(pkg-config --modversion "$pc" 2>/dev/null || echo "?")
    echo "OK  $pkg: $actual (pc=$pc, mínimo $version)"
  else
    echo "OK  $pkg: $(pkg-config --modversion "$pc") (pc=$pc)"
  fi
}

check_optional_pc() {
  local pc="$1" pkg="$2"
  if pkg-config --exists "$pc"; then
    echo "OK  $pkg (opcional): $(pkg-config --modversion "$pc")"
  else
    optional+=("$pkg (pc=$pc) — necessário só para backend udev/sessão real")
  fi
}

check_tool() {
  local tool="$1" pkg="$2"
  if command -v "$tool" >/dev/null 2>&1; then
    echo "OK  $tool: $($tool --version 2>&1 | head -1)"
  else
    missing+=("$pkg (ferramenta: $tool)")
  fi
}

echo "== Sistema =="
if [ -r /etc/os-release ]; then
  . /etc/os-release
  echo "Distro: $PRETTY_NAME"
fi

echo "== Ferramentas =="
check_tool cc build-essential
check_tool pkg-config pkg-config
check_tool cargo rustc
check_tool bun bun

echo "== Bibliotecas (pkg-config) =="
check_pc gtk4 libgtk-4-dev 4.0
check_pc webkitgtk-6.0 libwebkitgtk-6.0-dev 2.52
check_pc gtk4-layer-shell-0 libgtk4-layer-shell-dev 1.0
check_pc wayland-client libwayland-dev 1.20
check_pc wayland-server libwayland-dev 1.20
check_pc xkbcommon libxkbcommon-dev 1.0
check_pc libudev libudev-dev 249
check_pc libinput libinput-dev 1.19
check_optional_pc libdrm libdrm-dev
check_optional_pc gbm libgbm-dev
check_optional_pc libseat libseat-dev
check_optional_pc dbus-1 libdbus-1-dev
check_optional_pc pam pam

echo "== wayland-protocols =="
if [ -d /usr/share/wayland-protocols ]; then
  echo "OK  wayland-protocols: $(pkg-config --modversion wayland-protocols 2>/dev/null || echo 'presente')"
else
  missing+=("wayland-protocols")
fi

if [ "${#optional[@]}" -gt 0 ]; then
  echo
  echo "Opcionais ausentes (não bloqueiam cargo check com as features atuais):"
  printf '  - %s\n' "${optional[@]}"
fi

if [ "${#missing[@]}" -gt 0 ]; then
  echo
  echo "FALTAM dependências:"
  printf '  - %s\n' "${missing[@]}"
  echo
  echo "Instale (Ubuntu 26.04):"
  echo "  sudo apt-get install -y ${missing[*]} wayland-protocols dbus"
  exit 1
fi

echo
echo "Todas as dependências de build estão presentes."
