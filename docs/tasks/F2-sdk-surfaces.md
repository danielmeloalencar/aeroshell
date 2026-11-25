# Tarefa F2-sdk-surfaces + defineDesktop

> Texto aprovado pelo humano em 2026-11-25. Host genérico fechado em `bb592e5`.

## Objetivo (uma frase, verificável)
Tornar o SDK real para desenvolvedores web: um `desktop.config.tsx` cria background e painel somente pelo SDK, sem Rust, com pixels, z-order e input pass-through medidos.

## Contexto mínimo
- Plano mestre §3 (contrato), §4 (tree), regra 6 (JSDoc + exemplo).
- `packages/sdk/src/api/`, `packages/sdk/src/client.ts`, `packages/runtime/src/`.
- `crates/aeroshell-shell/src/surfaces/` e `crates/aeroshell-ipc/`.
- `tests/integration/f2_surface_host.py` e `f2_surface_perf.py`.

## Escopo
1. `packages/sdk/src/api/surfaces.ts` (runtime Bun): `aero.surfaces.create(kind, opts)/update/destroy/list/setInputRegion/setKeyboard`, tipado via ts-rs; emite comandos IPC ao host e consome `SurfaceCreated/Updated/Destroyed` do stream.
2. `packages/sdk/src/client.ts` (guest browser, zero dependência de framework): `createClient()` sobre `window.aero`, inscrição em eventos e update da PRÓPRIA superfície (input/blur region), utilizável sem adapter.
3. `packages/sdk/src/define.ts` + `packages/runtime/src/config-loader.ts`: `defineDesktop({surfaces, frame, layouts, rules, keybindings, idle, lock, greeter})` com tipo completo do §3; runtime aplica só `surfaces/frame/rules`, demais campos tipados e deferidos com erro explícito se usados. Carrega `desktop.config.tsx` do dev no startup.
4. Segurança: só runtime privilegiado cria superfícies; guest só atualiza a própria região/blur; só `aero://` (dev server fica no cartão HMR).
5. Tipos SOMENTE ts-rs (`generate:types`); JSDoc + `examples/blank-desktop` em TS puro (background + panel), provando uso com qualquer framework.

## Critério de aceite
1. Teste-first: fixture `desktop.config.tsx` cria background+panel só via SDK; baseline `VEREDITO: FALHA` (API ausente), depois `VEREDITO: OK` com pixels/z-order e pass-through transparente medidos.
2. `bash scripts/check.sh`, clippy `--features bench -- -D warnings` e `cargo check --workspace --no-default-features` verdes.
3. Vizinhos frame/policy/hot-reload/workspaces/launcher/host verdes; `frame_follows_resize` segue falha pré-existente por A/B.
4. `f2_surface_perf.py`: 1 WebProcess no host e 0 redraws estáticos.
5. Juiz visual: blank-desktop carregado do config TS + 2 `foot`, 60 s responsivo; humano responde `ok/aprovado`.

## Fora de escopo
HMR, adapters React/Solid/Vue, layouts/tiling (F3), keybindings, idle/lock/greeter (F5/F6), templates/CLI (F8), snap/F7, D-Bus de notificações e dev server.

## Limites
Um cartão por sessão; parar para ok visual; atualizar STATE.md; sem push; sem dependências novas sem aprovação; não tocar snap/F7/F1/lock/greeter.
