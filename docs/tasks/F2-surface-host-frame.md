# Tarefa F2-surface-host-frame — migrar o kind frame para o host genérico

> Fecha o F2-surface-host-genérico depois de F2-surface-perf-fix aprovado.

## Objetivo (uma frase, verificável)
`examples/frame_webview.rs` vira um shim fino para `aeroshell_shell::surfaces::frame`, sem dois caminhos de produção, preservando os harnesses de frame/policy/hot-reload/workspaces/launcher e a responsividade medida em F2-surface-perf-fix.

## Contexto mínimo
- `crates/aeroshell-shell/examples/frame_webview.rs` — spike com pool/related-view, canvas fixo, 9-slice, CSD e grabs.
- `crates/aeroshell-shell/src/surfaces/` — host genérico e WebContext compartilhado.
- ADRs 0001 (related-view) e 0007 (canvas fixo/9-slice/frame_painted serial).
- `tests/integration/frame_buttons.py`, `frame_resize_glue.py`, `f1_csd_gtk.py`, `f2_full_decoration_policy.py`, `f2_hotreload_decoration_policy.py`, `f2_workspaces.py`, `f2_launcher.py`.
- `/tmp/opencode/f2-surface-fix2-perf-final.log` — baseline pós-perf-fix.

## Escopo
1. Lógica do spike vai para `crates/aeroshell-shell/src/surfaces/frame.rs` usando bridge, aero://, shell-reset.css e WebContext único/âncora/pool.
2. PRESERVAR ADR 0007, política TS/hot-reload/CSD flash-less (opacidade), título/ícone via stream, cursores de borda, grabs move/resize, insets/titlebar-slices do `aero_frame_v1`.
3. `examples/frame_webview.rs` vira shim fino (sem dois caminhos de produção).
4. Harness novo `tests/integration/f2_surface_host_frame.py` com baseline FALHA antes da migração e OK depois.
5. Re-executar `f2_surface_perf`: 1 WebProcess, 0 redraws estáticos e p99 estável por 60 s.

## Critério de aceite
1. Baseline: `python3 tests/integration/f2_surface_host_frame.py` → `VEREDITO: FALHA` antes; depois → `VEREDITO: OK`.
2. Harnesses de frame/policy/hot-reload/workspaces/launcher → OK, exceto `frame_follows_resize` cuja falha pré-existente exige A/B.
3. `f2_surface_perf.py`: 1 WebProcess real no host, 0 redraws estáticos, p99 não degrada em 60 s.
4. `bash scripts/check.sh`, clippy bench `-D warnings`, `--no-default-features` verdes.
5. Juiz visual: `foot`/`gedit`/`kate` com decoração, botões, cursores, hot-reload e 60 s responsivo; humano responde "ok/aprovado".

## Fora de escopo
Snap/F7/F1, lock/greeter, novas dependências, dois caminhos de produção, correção de `frame_follows_resize` (pré-existente por A/B). Depois: F2-sdk-surfaces + defineDesktop (NÃO iniciar).

## Limites
Uma tarefa por sessão. Sem push. STATE.md atualizado. Sem dependências novas.
