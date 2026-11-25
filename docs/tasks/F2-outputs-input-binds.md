# Tarefa F2-outputs+input-binds — contrato de outputs e mecanismo de atalhos

> Rascunho criado do texto fornecido pelo humano em 2026-11-25. Base: `2ab2adb` (F2-HMR fechado). Implementação autorizada por “ok, implemente”.
> FECHADO em 2026-11-25: integração/vizinhos/gates/3 perfs verdes e juiz visual “sim, tudo funcionou corretamente… está aprovado”, com Super+1/2 injetados porque o GNOME captura os atalhos físicos.

## Objetivo (uma frase, verificável)
Expor outputs reais e binds globais tipados no SDK/runtime, com grabs e `KeyBind` no compositor e política/callbacks no `desktop.config.tsx`, provados por input sintético e por juiz visual em dois outputs.

## Contexto mínimo
- `crates/aeroshell-ipc/src/types.rs` — única fonte dos novos tipos TS, gerados por `bun run generate:types`.
- `crates/aeroshell-compositor/src/{state.rs,window_ipc.rs,input/,backend/}` — estado de outputs, IPC privilegiado e caminho de teclado existente.
- `packages/sdk/src/api/{outputs,input}.ts`, `packages/sdk/src/{index,client,store}.ts` — APIs tipadas e eventos; guest continua sem privilégio global.
- `packages/runtime/src/{config-loader,keybindings,ipc-client}.ts` — registro/diff dos binds e despacho da política do desenvolvedor.
- `examples/blank-desktop/` — dois binds e painel por output, sem framework e com JSDoc.
- `tests/integration/f2_outputs_input_binds.py` — harness principal com `AERO_TEST_INPUT_SOCK`.
- Plano mestre §3/§4; fronteiras em `docs/architecture.md`. Nenhum XML/ADR novo.

## Escopo
1. `aero.outputs.list/get` e eventos `OutputAdded/Changed/Removed` (ts-rs) a partir do estado existente de winit multi-output/headless: `geometry`, `scale`, `mode`, `name`; superfícies por output continuam usando o suporte existente.
2. O compositor registra grabs de teclado recebidos da config e emite `KeyBind` ao runtime; o runtime executa callback do desenvolvedor ou ação tipada do SDK. `aero.input.bind/unbind/list` usa somente o IPC privilegiado existente.
3. `defineDesktop` aceita `keybindings`; `layouts`, `idle`, `lock` e `greeter` continuam com rejeição explícita.
4. Somente o runtime privilegiado registra/remove grabs globais; o guest da WebView não pode fazê-lo, inclusive tentando comandos pela ponte restrita.
5. Tipos somente por ts-rs; JSDoc e `blank-desktop` com Super+1/Super+2 e painel por output.
6. Hotplug dinâmico udev fica registrado como dívida e NÃO é implementado.

## Critério de aceite
1. Teste-first: `python3 tests/integration/f2_outputs_input_binds.py` inicia com `VEREDITO: FALHA` por API/mecanismo ausentes; depois, config Super+1/Super+2 dispara callback/ação via `AERO_TEST_INPUT_SOCK`, bind/unbind/list funcionam e outputs batem com nome, geometria, escala e modo do backend → `VEREDITO: OK`.
2. O mesmo harness prova create/update/reload da config sem binds duplicados, eventos de output coerentes com o backend e recusa de registro global pelo guest; layouts/idle/lock/greeter continuam rejeitados.
3. `bash scripts/check.sh`, `cargo clippy --workspace --all-targets --features bench -- -D warnings` e `cargo check --workspace --no-default-features` verdes.
4. Vizinhos `f2_surface_host_frame`, `frame_resize_glue`, `f1_csd_gtk`, `f2_full_decoration_policy`, `f2_hotreload_decoration_policy`, `f2_workspaces`, `f2_launcher`, `f2_surface_host`, `f2_sdk_surfaces` e `f2_hmr`: todos `VEREDITO: OK`.
5. Três execuções de `python3 tests/integration/f2_surface_perf.py`: 1 WebProcess no host inclusive com 4 superfícies, 0 redraws estáticos/5 s e 0 px de damage headless; todas as amostras preservadas e ressalva de VM sem 3D.
6. Juiz visual: pressionar Super+1/Super+2 e ver as ações; conferir painéis/superfícies nos 2 outputs e operar por 60 s; humano responde `ok/aprovado`. Aplicação fica aberta para o humano fechar.

## Comandos de validação
```
bun run generate:types
bun build packages/sdk/src/client.ts --target=browser --outfile=packages/sdk/dist/client.js
cargo build --workspace --examples
cargo build -p aeroshell-shell
cargo build -p aeroshell-compositor --features bench
python3 tests/integration/f2_outputs_input_binds.py
bash scripts/check.sh
cargo clippy --workspace --all-targets --features bench -- -D warnings
cargo check --workspace --no-default-features
python3 tests/integration/f2_surface_perf.py
```

## Fora de escopo
Gestos (F7); layouts/tiling (F3); idle/lock/greeter (F5/F6); hotplug udev; adapters React/Solid/Vue e exemplos Three.js; snap/F7; novas ações de janela fora das necessárias aos dois binds; alterações de protocolo/ADR; navegação web geral.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (sintoma, onde, hipótese) e VOLTE. Só investigue desvio bloqueante, por no máximo 15 minutos.

## Limites
Um cartão por sessão; parar para o juiz visual; sem push; sem dependências novas; não alterar protocolos/ADRs/lock/greeter; preservar a edição visual não commitada do usuário; regra das duas tentativas; duas horas sem resultado visível → parar e registrar bloqueio. Atualizar `docs/STATE.md` §3/§4/§5/§6/§7/§9 e `docs/RUNBOOK.md`.
