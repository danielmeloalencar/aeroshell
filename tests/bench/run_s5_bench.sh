#!/usr/bin/env bash
# S5 sem protocolo: PSS por processo, estabilização por polling (<2% em 3 leituras).
# Nenhuma amostra falha é descartada em silêncio: falha => nova tentativa registrada.
# O compositor NÃO entra na soma (apenas aeroshell-shell + filhos WebKit).
set -uo pipefail
cd "$(dirname "$0")/../.."

export LD_LIBRARY_PATH=/tmp/opencode/aptroot/usr/lib/x86_64-linux-gnu
BIN=target/release/aeroshell-shell
COMP=target/release/aeroshell-compositor
OUT=/tmp/opencode/s5-bench-results-v2.txt
: > "$OUT"
log() { echo "$*" | tee -a "$OUT"; }

PROCS=aeroshell-shell,WebKitWebProcess,WebKitNetworkProcess,WebKitGPUProcess

pss_sum() {
  for pid in $(ps -C "$PROCS" -o pid=); do
    grep '^Pss:' /proc/$pid/smaps_rollup 2>/dev/null
  done | awk '{s+=$2} END {print s+0}'
}

print_procs() {
  for pid in $(ps -C "$PROCS" -o pid=); do
    local comm pss
    comm=$(cat /proc/$pid/comm 2>/dev/null || echo '?')
    pss=$(grep '^Pss:' /proc/$pid/smaps_rollup 2>/dev/null | awk '{print $2}')
    log "pid=$pid comm=$comm Pss_kB=${pss:-0}"
  done
  log "WebKitWebProcess=$(ps -C WebKitWebProcess --no-headers | wc -l) WebKitNetworkProcess=$(ps -C WebKitNetworkProcess --no-headers | wc -l) WebKitGPUProcess=$(ps -C WebKitGPUProcess --no-headers | wc -l)"
}

wait_stable() {
  local label="$1"
  local start prev=0 consec=0 total i
  local started=$(date +%s)
  for i in $(seq 1 80); do
    total=$(pss_sum)
    if [ "$total" -le 0 ]; then
      log "AMOSTRA_VAZIA $label tentativa=$i (sem processos) — aguardando"
      consec=0
      prev=0
      sleep 0.5
      continue
    fi
    if [ "$prev" -gt 0 ]; then
      local diff=$(( total > prev ? total - prev : prev - total ))
      if [ $(( diff * 100 )) -lt $(( prev * 2 )) ]; then
        consec=$((consec+1))
      else
        consec=1
      fi
    else
      consec=1
    fi
    prev=$total
    if [ "$consec" -ge 3 ]; then
      local elapsed=$(( $(date +%s) - started ))
      log "ESTABILIZOU $label PSS_kB=$total leituras=3 variacao<2% tempo_s=$elapsed tentativas=$i"
      print_procs
      return 0
    fi
    sleep 0.5
  done
  log "FALHA $label: PSS não estabilizou em 80 leituras — tentativa será repetida"
  return 1
}

run_once() {
  local label="$1" n="$2" related="$3" atlas="$4" rep="$5" attempt="$6"
  local tag="s5v2-$(echo "$label" | tr ' ' '-')-$rep-t$attempt"
  log "===== $label rep=$rep tentativa=$attempt ====="
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
  env "${env_vars[@]}" timeout 18s "$BIN" > "/tmp/opencode/$tag-shell.log" 2>&1 &
  local shell_pid=$!
  local ready='BENCH A: load-changed=Finished de TODAS as views'
  [ "$atlas" = 1 ] && ready='BENCH: medição enviada pela WebView'
  for _ in $(seq 1 100); do
    grep -q "$ready" "/tmp/opencode/$tag-shell.log" && break
    sleep 0.2
  done
  grep -q "$ready" "/tmp/opencode/$tag-shell.log" || log "AVISO $label rep=$rep: sinal de pronto não encontrado"
  local ok=0
  wait_stable "$label rep=$rep tentativa=$attempt" && ok=1
  wait "$shell_pid" 2>/dev/null || true
  kill "$comp" 2>/dev/null || true
  sleep 0.5
  log "---- tempos ($label rep=$rep tentativa=$attempt) ----"
  grep -E 'BENCH A: WebViews criadas|BENCH A: WebViews destruídas|BENCH: medição' "/tmp/opencode/$tag-shell.log" | tee -a "$OUT" || true
  return $((1-ok))
}

run_case() {
  local label="$1" n="$2" related="$3" atlas="$4"
  for rep in 1 2 3; do
    local attempt=1
    while [ "$attempt" -le 3 ]; do
      if run_once "$label" "$n" "$related" "$atlas" "$rep" "$attempt"; then
        break
      fi
      log "REPETINDO $label rep=$rep (tentativa $attempt falhou)"
      attempt=$((attempt+1))
    done
  done
}

run_case "N1-sem-related" 1 0 0
run_case "N5-sem-related" 5 0 0
run_case "N20-sem-related" 20 0 0
run_case "N1-related" 1 1 0
run_case "N5-related" 5 1 0
run_case "N20-related" 20 1 0
run_case "atlas-1webview-20frames" 0 0 1

log "RESULTADOS em $OUT"
