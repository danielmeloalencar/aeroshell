# Tarefa F2-adapters+exemplos — any-framework e desktop inteiro

> Cartão fornecido e implementação autorizada pelo humano em 2026-11-26. Base: `e453ec8`
> (F2-alpha-confirm fechado). Último cartão da F2. Uma tarefa por sessão; PARAR para o juiz visual.

## Objetivo (uma frase, verificável)
Provar que React, Solid, Vue e Three.js montam um desktop inteiro sobre adapters finos do store do SDK,
usando `defineDesktop` + HMR e sem qualquer código Rust escrito pelo desenvolvedor do exemplo.

## Contexto mínimo (arquivos que importam)
- `docs/plano-mestre.md` §3 — adapters com `useSyncExternalStore`, signals e refs; regra 6 (DX).
- `packages/sdk/src/{index,client,store,define}.ts` e `src/api/` — fonte única de estado e comandos.
- `packages/react/src/` — adapter React atual; `packages/solid/` e `packages/vue/` — novos adapters.
- `packages/runtime/src/{config-loader,dev-server,keybindings}.ts` — configuração, HMR e binds.
- `examples/blank-desktop/` — base sem framework a migrar para React sem perder o vizinho existente.
- `tests/integration/{f2_hmr,f2_alpha_confirm,f2_surface_perf}.py` — integração e invariantes.

## Escopo
1. Adapters finos em `@aeroshell/react`, `@aeroshell/solid` e `@aeroshell/vue`:
   `useWindows/useWorkspaces/useOutputs/useApps/useEvent/useSurface` e provider/mount de superfície.
   React usa `useSyncExternalStore`; Solid, signals; Vue, refs/computed. `/lock` e `/greeter` ficam para F5/F6.
2. Exemplos `blank-desktop` (React), `win11-like` (taskbar centrada + start menu, Vue ou React) e
   `threejs-desktop` (cena 3D de fundo + widgets transparentes com input pass-through), todos via
   `defineDesktop` + HMR e zero Rust do desenvolvedor.
3. Tipos de estado somente ts-rs; JSDoc e exemplo em toda API pública nova.
4. Teste-first por adapter: abrir janela atualiza o hook e um bind dispara a ação. Three.js: pixels
   mudam durante animação, área transparente repassa input e cena estática fica ociosa.

## Critério de aceite (comandos exatos + resultado esperado)
1. Baseline antes da implementação: `bun test packages/react packages/solid packages/vue` FALHA por
   exports/adapters ausentes; o harness dos exemplos FALHA antes das novas APIs/cenas.
2. Depois: `bun test packages/react packages/solid packages/vue` verde, com reatividade e cleanup
   provados nos três adapters e bind → ação coberto sem DOM/jsdom/happy-dom.
3. `python3 tests/integration/f2_adapters_examples.py` → `VEREDITO: OK`: blank e win11 reativos/HMR;
   Three.js tem pixels distintos em dois instantes, input transparente chega ao cliente e, com a animação
   parada, não há redraws estáticos no intervalo medido.
4. `bash scripts/check.sh` verde; `cargo clippy --workspace --all-targets --features bench -- -D warnings`
   e `cargo check --workspace --no-default-features` verdes.
5. Os 15 vizinhos do F2-alpha-confirm (`frame_resize_glue`, `f2_surface_host_frame`, `f1_csd_gtk`,
   `f2_full_decoration_policy`, `f2_hotreload_decoration_policy`, `f2_workspaces`, `f2_launcher`,
   `f2_surface_host`, `f2_sdk_surfaces`, `f2_hmr`, `f2_outputs_input_binds`, `f2_windows_completo`,
   `f2_alpha_confirm`, `frame_buttons`, `headless_render_damage`) são reexecutados e seus resultados crus
   registrados; nenhuma falha é escondida.
6. `python3 tests/integration/f2_surface_perf.py`: 1 WebProcess no host, inclusive com 4 superfícies,
   0 redraws estáticos/5 s e redraws somente por damage; três amostras preservadas, com ressalva de VM sem 3D.
7. Gate S3 release reexecutado: `AERO_TARGET=release python3 tests/integration/f2_alpha_confirm.py` →
   `VEREDITO: OK`, p99 <1 ms, readback 1×1 e `full_frame=0`.
8. Juiz visual: Three.js girando + taskbar win11 + blank, cada um com HMR e 60 s responsivo; clique opaco
   captura e transparente repassa. Aplicação deixada aberta e aprovação explícita do humano.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Layouts/tiling/overview (F3), idle/lock (F5), greeter (F6), snap/screencopy (F7).
- Templates/CLI/.deb (F8), CSP/hardening (F9), hotplug e otimização de move.
- `/lock` e `/greeter` nos adapters; mudanças em protocolos, ADRs aceitos ou código Rust de produto.

## Limites
- Tempo: 2 h sem resultado visível → parar e registrar o bloqueio; regra das duas tentativas.
- Dependências novas exigem aprovação explícita com nome, versão, licença, motivo e alternativa.
  Proposta ainda NÃO aplicada: `solid-js@2.0.4` (MIT), `@solidjs/web@2.0.4` (MIT, `render`/mount),
  `vue@3.5.46` (MIT) e `three@0.187.0` (MIT); `three` é dependência exclusiva do exemplo.
  Sem jsdom/happy-dom, `@types/three` (entry Three em `.js`) ou dependências de produção Rust.
- Arquivos protegidos: AGENTS.md §6. Sem push; commits locais somente com autorização.

## Entrega (relatório curto)
Baseline e depois com o mesmo comando · gates/vizinhos/perf/S3 · o que não foi testado · juiz visual
pendente/OK · próximo passo proposto, não executado. Atualizar `docs/STATE.md` §3/§4/§5/§6/§7/§9
e `docs/RUNBOOK.md`; após aprovação, commit `docs(f2)` de fechamento com números e deferidos F3+.
