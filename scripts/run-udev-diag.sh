#!/usr/bin/env bash
# AeroShell em sessão real (backend udev/DRM/GBM/libseat) com diagnóstico ligado.
# Uso no tty3:  cd ~/Projetos/aeroshell && ./scripts/run-udev-diag.sh [args do compositor]
# Ou, mais curto (atalho criado em $HOME/u):  ~/u [args do compositor]
# Exemplo: ~/u --scale 1.5  (F1f: os args são repassados ao compositor)
#
# Encerre com Ctrl+C. Trocar de VT e fechar a aplicação são ações do humano.
# O log fica em $HOME/aeroshell-udev-YYYYMMDD-HHMMSS.log — envie-o após reiniciar.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$REPO/target/debug/aeroshell-compositor"
SHELL_BIN="$REPO/target/debug/examples/frame_webview"
PANEL_BIN="$REPO/target/debug/aeroshell-shell"   # painel React/TSX (layer surface)
SOCKET="wayland-aero-tty3"

cd "$REPO"

LOG="$HOME/aeroshell-udev-$(date +%Y%m%d-%H%M%S).log"
exec > >(tee -a "$LOG") 2>&1

echo "LOG: $LOG"
echo "=== AeroShell DRM (udev) — diagnóstico F1d3 ==="
echo "Repositório: $REPO"
echo

echo "[1/3] Compilando (com --features bench: sondas FOOT_COMMIT/UDEV_FRAME_CB)..."
# F1d3: sem `bench` as sondas de commit/frame-callback NÃO existem no binário e
# o log fica sem evidência — isso aconteceu em 2026-11-22 (log 051134).
cargo build --workspace --features bench
cargo build -p aeroshell-shell --example frame_webview
cargo build -p aeroshell-shell   # painel TSX (separado do frame_webview)

echo "[2/3] Verificando ambiente..."
[ -x "$BIN" ] || { echo "ERRO: $BIN ausente"; exit 1; }
[ -x "$SHELL_BIN" ] || { echo "ERRO: $SHELL_BIN ausente"; exit 1; }
[ -x "$PANEL_BIN" ] || { echo "ERRO: $PANEL_BIN ausente"; exit 1; }
[ -e /dev/dri/card0 ] || { echo "ERRO: /dev/dri/card0 ausente"; exit 1; }
[ -e /dev/dri/renderD128 ] || { echo "ERRO: /dev/dri/renderD128 ausente"; exit 1; }
if [ -z "${XDG_RUNTIME_DIR:-}" ]; then
  echo "ERRO: XDG_RUNTIME_DIR vazio — faça login no tty (não use 'su' sem '-')"
  exit 1
fi
unset WAYLAND_DISPLAY DISPLAY

# O painel React lista janelas via IPC; sem esta variável o compositor não cria
# o socket e o bridge devolve `lista=` vazia (log 053838). PID evita colisão
# com instâncias anteriores; o cleanup abaixo remove o arquivo.
IPC_SOCK="$XDG_RUNTIME_DIR/wayland-aero-tty3-ipc-$$"
rm -f "$IPC_SOCK" 2>/dev/null || true

echo "[3/3] Subindo o compositor (RUST_LOG=debug)..."
echo "  socket: $SOCKET"
echo "  log:    $LOG"
echo
echo "O QUE OLHAR:"
echo "  1. Aparecem o painel TSX e a janela 'foot'."
echo "  2. Arrastar a barra move SOMENTE a janela dona."
echo "  3. Ctrl+Alt+F2 sai do VT; Ctrl+Alt+F3 volta (implementado no compositor)."
echo "  4. Cursor ainda NÃO é desenhado no backend udev — o ponteiro move e clica,"
echo "     mas fica invisível (dívida registrada)."
echo
echo "Ctrl+C para sair. Se travar: reinicie e envie $LOG"
echo "------------------------------------------------------------"
echo

# RUST_LOG=debug é OBRIGATÓRIO: sem ele as linhas UDEV_FRAME/UDEV_VBLANK não
# aparecem e o analisador udev_frame_probe.py dá FALHA pelo motivo errado.
# LIBGL_ALWAYS_SOFTWARE=1: VM VirtualBox sem 3D (desempenho aqui não vale).
set +e
env \
  RUST_LOG=debug \
  XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
  LIBGL_ALWAYS_SOFTWARE=1 \
  AERO_WINDOW_IPC_SOCK="$IPC_SOCK" \
  "$BIN" \
    --backend udev \
    --socket "$SOCKET" \
    --shell "$SHELL_BIN" \
    --shell-env LD_PRELOAD=libgtk4-layer-shell.so.0 \
    --command foot \
    "$@" &
comp_pid=$!

# O painel TSX e um cliente PUBLICO (nao o --shell do compositor): espera o
# socket e conecta por WAYLAND_DISPLAY, como em tests/integration/react_panel.py.
SOCK_PATH="$XDG_RUNTIME_DIR/$SOCKET"
for _ in $(seq 1 100); do
  if [ -S "$SOCK_PATH" ]; then break; fi
  sleep 0.1
done
panel_pid=""
if [ -S "$SOCK_PATH" ]; then
  env \
    WAYLAND_DISPLAY="$SOCKET" \
    XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
    GDK_BACKEND=wayland \
    GTK4_LAYER_SHELL_LIB=libgtk4-layer-shell.so.0 \
    AERO_WINDOW_IPC_SOCK="$IPC_SOCK" \
    RUST_LOG=debug \
    "$PANEL_BIN" &
  panel_pid=$!
  echo "painel TSX iniciado pid=$panel_pid"
else
  echo "ERRO: socket $SOCK_PATH nao apareceu; painel nao iniciado"
fi

# Encerra SOMENTE os PIDs que este script iniciou (AGENTS.md 0.5).
cleanup_children() {
  if [ -n "$panel_pid" ] && kill -0 "$panel_pid" 2>/dev/null; then
    kill "$panel_pid" 2>/dev/null || true
    wait "$panel_pid" 2>/dev/null || true
  fi
  rm -f "$IPC_SOCK" 2>/dev/null || true
}
trap cleanup_children EXIT

wait "$comp_pid"
status=$?
cleanup_children
trap - EXIT
set -e

echo
echo "------------------------------------------------------------"
if [ "$status" -ne 0 ]; then
  echo "Compositor saiu com EXIT=$status."
  echo "Se a mensagem for 'Resource temporarily unavailable' em session.open(),"
  echo "você NÃO está num VT de login ativo: use Ctrl+Alt+F3, faça login e tente de novo."
else
  echo "Compositor encerrado (EXIT=0)."
fi
exit "$status"
