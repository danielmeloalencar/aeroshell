#!/usr/bin/env bash
# S5 sem protocolo: 3 repetições por config, PSS por processo após estabilização.
# O compositor NÃO entra na soma (apenas aeroshell-shell + filhos WebKit).
set -uo pipefail
cd "$(dirname "$0")/../.."

export LD_LIBRARY_PATH=/tmp/opencode/aptroot/usr/lib/x86_64-linux-gnu
BIN=target/release/aeroshell-shell
COMP=target/release/aeroshell-compositor
OUT=/tmp/opencode/s5-bench-results.txt
: > "$OUT"

sample() {
  local label="$1" rep="$2" log="$3"
  echo "===== $label rep=$rep =====" | tee -a "$OUT"
  for pid in $(ps -C aeroshell-shell,WebKitWebProcess,WebKitNetworkProcess,WebKitGPUProcess -o pid=); do
    local comm pss
    comm=$(cat /proc/$pid/comm 2>/dev/null || echo '?')
    pss=$(grep '^Pss:' /proc/$pid/smaps_rollup 2>/dev/null | awk '{print $2}')
    echo "pid=$pid comm=$comm Pss_kB=${pss:-0}" | tee -a "$OUT"
  done
  local total
  total=$(for pid in $(ps -C aeroshell-shell,WebKitWebProcess,WebKitNetworkProcess,WebKitGPUProcess -o pid=); do grep '^Pss:' /proc/$pid/smaps_rollup 2>/dev/null; done | awk '{s+=$2} END {print s+0}')
  echo "PSS_SOMA_kB=$total" | tee -a "$OUT"
  echo "WebKitWebProcess=$(ps -C WebKitWebProcess --no-headers | wc -l) WebKitNetworkProcess=$(ps -C WebKitNetworkProcess --no-headers | wc -l) WebKitGPUProcess=$(ps -C WebKitGPUProcess --no-headers | wc -l)" | tee -a "$OUT"
}

run_case() {
  local label="$1" n="$2" related="$3" atlas="$4"
  for rep in 1 2 3; do
    local tag="s5-$(echo "$label" | tr ' ' '-')-$rep"
    RUST_LOG=error "$COMP" --backend winit --socket "wayland-$tag" > "/tmp/opencode/$tag-comp.log" 2>&1 &
    local comp=$!
    sleep 2
    local -a env_vars=(WAYLAND_DISPLAY="wayland-$tag" GDK_BACKEND=wayland LD_PRELOAD=/tmp/opencode/aptroot/usr/lib/x86_64-linux-gnu/libgtk4-layer-shell.so.0 RUST_LOG=info)
    if [ "$atlas" = 1 ]; then
      env_vars+=(AERO_BENCH_ATLAS=1)
    else
      env_vars+=("AERO_BENCH=$n")
      [ "$related" = 1 ] && env_vars+=(AERO_BENCH_RELATED=1)
    fi
    env "${env_vars[@]}" timeout 14s "$BIN" > "/tmp/opencode/$tag-shell.log" 2>&1 &
    local shell_pid=$!
    local ready='BENCH A: load-changed=Finished de TODAS as views'
    [ "$atlas" = 1 ] && ready='BENCH B: atlas'
    for _ in $(seq 1 80); do
      grep -q "$ready" "/tmp/opencode/$tag-shell.log" && break
      sleep 0.2
    done
    sleep 3
    sample "$label" "$rep" "/tmp/opencode/$tag-shell.log"
    sleep 0.5
    sample "$label" "$rep-estabilizado" "/tmp/opencode/$tag-shell.log"
    wait "$shell_pid" 2>/dev/null || true
    kill "$comp" 2>/dev/null || true
    sleep 0.5
    echo "---- tempos ($label rep=$rep) ----" | tee -a "$OUT"
    grep -E 'BENCH A: WebViews criadas|BENCH A: WebViews destruídas|BENCH/METRICS' "/tmp/opencode/$tag-shell.log" | tee -a "$OUT"
  done
}

run_case "N1-sem-related" 1 0 0
run_case "N5-sem-related" 5 0 0
run_case "N20-sem-related" 20 0 0
run_case "N1-related" 1 1 0
run_case "N5-related" 5 1 0
run_case "N20-related" 20 1 0
run_case "atlas-1webview-20frames" 0 0 1

echo "RESULTADOS em $OUT"
