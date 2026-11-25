# Tarefa F2-HMR via Bun.serve — loop de DX do desenvolvedor web

> Texto fornecido e implementação autorizada pelo humano em 2026-11-25. Base: `6f989a4` (F2-sdk-surfaces fechado).

## Objetivo (uma frase, verificável)
Em `--dev`, editar `desktop.config.tsx` e os módulos das superfícies atualiza DOM/pixels e o diff create/update/destroy sem reiniciar compositor/runtime nem recriar WebView/superfície ou aumentar WebProcesses; produção continua restrita a `aero://`.

## Contexto mínimo
- `packages/runtime/src/{main,config-loader,ipc-client}.ts` — startup e aplicação declarativa atual.
- `packages/runtime/src/dev-server.ts` — novo servidor exclusivo de desenvolvimento.
- `packages/sdk/src/client.ts` e `packages/sdk/src/api/surfaces.ts` — guest e tipos ts-rs/wire.
- `crates/aeroshell-shell/src/{bridge.rs,scheme.rs,surfaces/mod.rs}` — allowlist, navegação e atualização sem recriar WebView.
- `crates/aeroshell-ipc/src/types.rs` — única fonte de novos tipos TS, gerados por `bun run generate:types`.
- `examples/blank-desktop/` — exemplo editável ao vivo, sem framework e com JSDoc.
- `tests/integration/{f2_sdk_surfaces,f2_surface_perf}.py` — harnesses e invariantes reutilizáveis.
- ADR 0001 (WebProcess compartilhado) e ADR 0003 (não criar rota própria de blur).

## Escopo
1. `Bun.serve` somente loopback e somente com `--dev`: build dos módulos das superfícies pelo `Bun.build`/bundler já existente, assets same-origin e WebSocket de reload; nenhuma dependência nova.
2. Host aceita somente a origem loopback explicitamente configurada em dev; sem `--dev` rejeita HTTP(S), e nenhum modo permite navegação externa. CSP permanece F9.
3. Guest possui canal HMR opt-in; reload acontece na WebView existente. O loader observa config e dependências, reimporta sem cache e aplica diff determinístico de create/update/destroy sem restart.
4. Tipos novos somente por ts-rs; JSDoc e `blank-desktop` editável ao vivo.

## Critério de aceite
1. Teste-first: `python3 tests/integration/f2_hmr.py` começa com `VEREDITO: FALHA` por servidor/canal HMR ausentes. Depois, ao editar cor e texto no TSX, o mesmo compositor/runtime mantém PIDs, DOM/pixels mudam, a identidade da layer/WebView é preservada e a contagem de WebProcesses não cresce → `VEREDITO: OK`.
2. O mesmo harness prova create/update/destroy por diff e segurança: sem `--dev`, URL HTTP é recusada; em dev só a origem loopback configurada é aceita e origem externa/navegação externa são recusadas.
3. `bash scripts/check.sh`, `cargo clippy --workspace --all-targets --features bench -- -D warnings` e `cargo check --workspace --no-default-features` verdes.
4. Vizinhos `f2_surface_host_frame`, `frame_resize_glue`, `f1_csd_gtk`, `f2_full_decoration_policy`, `f2_hotreload_decoration_policy`, `f2_workspaces`, `f2_launcher`, `f2_surface_host` e `f2_sdk_surfaces`: todos `VEREDITO: OK`.
5. `python3 tests/integration/f2_surface_perf.py`: 1 WebProcess no host, inclusive com 4 superfícies, 0 redraws estáticos/5 s e p99 sem regressão relevante; todas as amostras preservadas e ressalva de VM sem 3D.
6. Juiz visual: com 2 `foot`, editar cor/texto do `blank-desktop`, ver a mudança ao vivo e operar por 60 s; humano responde `ok/aprovado`. Aplicação fica aberta para o humano fechar.

## Comandos de validação
```
bun run generate:types
bun build packages/sdk/src/client.ts --target=browser --outfile=packages/sdk/dist/client.js
cargo build --workspace --examples
cargo build -p aeroshell-shell
cargo build -p aeroshell-compositor --features bench
python3 tests/integration/f2_hmr.py
bash scripts/check.sh
cargo clippy --workspace --all-targets --features bench -- -D warnings
cargo check --workspace --no-default-features
python3 tests/integration/f2_surface_perf.py
```

## Fora de escopo
Adapters; layouts/tiling e keybindings (F3); idle/lock/greeter (F5/F6); templates/CLI/bundle de produção (F8); snap/F7; CSP (F9); D-Bus de notificações; correção de `frame_follows_resize`; HMR do frame privilegiado; navegação web geral.

## Limites
Um cartão por sessão; parar para o juiz visual; sem push; sem dependências novas; não alterar protocolos/ADRs/lock/greeter; regra das duas tentativas; duas horas sem resultado visível → parar e registrar bloqueio. Atualizar `docs/STATE.md` §3/§4/§5/§6/§7/§9 e `docs/RUNBOOK.md`.
