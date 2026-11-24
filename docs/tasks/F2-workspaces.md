# Tarefa F2 — Workspaces

> Cartão aprovado pelo humano em 2026-11-24. Uma tarefa por sessão; o juiz visual fecha o critério visual.

## Objetivo (uma frase, verificável)
Gerenciar workspaces pelo compositor, SDK Bun e painel React, consumindo o stream `AeroEvent`, de modo que criar, ativar, renomear, mover janelas e destruir workspaces vazios atualize o estado e a visibilidade das janelas sem destruí-las.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-ipc/src/types.rs` — `AeroEvent` e tipos compartilhados com bindings ts-rs.
- `crates/aeroshell-compositor/src/state.rs` — estado das janelas e do compositor.
- `crates/aeroshell-compositor/src/window_ipc.rs` — comandos IPC e stream de eventos existentes.
- `crates/aeroshell-compositor/src/render/` e `src/backend/winit.rs` — visibilidade das janelas na cena.
- `packages/sdk/` — API pública e store reativo.
- `packages/panel/src/` — painel React existente.
- `tests/integration/f2_workspaces.py` — prova ponta a ponta a criar antes da implementação.

## Mecanismo exigido
1. Compositor mantém lista de workspaces, workspace ativa e janelas por workspace; publica `WorkspaceCreated`, `WorkspaceActivated`, `WorkspaceDestroyed` e `WindowMovedToWorkspace` no stream existente.
2. SDK expõe `listWorkspaces()`, `createWorkspace(name?)`, `activateWorkspace(id)`, `moveWindowToWorkspace(windowId, workspaceId)` e `destroyWorkspace(id)`; destruir recusa o último workspace ou um workspace com janelas. A edição de nome usa o mesmo mecanismo IPC tipado.
3. Painel lista workspaces, permite editar nomes, cria pelo botão `+` e ativa por clique.
4. Ao ativar, somente as janelas daquele workspace são renderizadas; janelas ocultas continuam mapeadas/vivas. Mover uma janela atualiza sua visibilidade conforme o workspace ativo.

## Critério de aceite (comandos exatos + resultado esperado)
1. Baseline antes da implementação: `python3 tests/integration/f2_workspaces.py` → `VEREDITO: FALHA` por ausência de workspaces; a saída crua é preservada.
2. Depois: `python3 tests/integration/f2_workspaces.py` → `VEREDITO: OK`; criar aparece na lista, ativar mostra só as janelas daquele workspace, mover remove de um e adiciona ao outro, destruir vazio remove da lista e as recusas são verificadas.
3. `bun test` → testes novos e existentes verdes, incluindo eventos e comandos do SDK/store.
4. `bash scripts/check.sh` → `OK: scripts/check.sh passou` em uma única execução completa.
5. `cargo clippy --workspace --all-targets --features bench -- -D warnings` → exit 0.
6. `cargo check --workspace --no-default-features` → exit 0.
7. Vizinhos de frame/CSD/policy/hot-reload continuam OK, conforme os harnesses aplicáveis do RUNBOOK.
8. Juiz visual: 2 workspaces e 2 `foot` em workspaces diferentes; alternar entre eles sem flicker, com o painel atualizado. Requer “ok” explícito do humano; a aplicação fica aberta para ele fechar.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Drag-and-drop de janelas entre workspaces (cartão próprio).
- Layouts/tiling por workspace (F3 ou posterior).
- Snap/F7/F1.
- Persistência de workspaces entre reinícios, atalhos de teclado e protocolo Wayland novo.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa. Não investigue desvios por mais de 15 minutos, salvo bloqueio do aceite.

## Limites
- Tempo: cerca de 2 horas sem resultado visível → pare e descreva o bloqueio.
- Dependências novas: nenhuma.
- Arquivos protegidos (`protocols/*.xml`, ADRs, segurança, toolchain/lock): não alterar.
- Sem push. Parar antes do fechamento para o juiz visual.

## Entrega (relatório curto)
O que funciona · comandos e saídas cruas · como testar à mão · o que não foi testado · descobertas laterais · 3 riscos de conclusão · próximo passo sem executá-lo. Atualizar `docs/STATE.md` e deixar handoff suficiente para retomada.

---
**Fechamento (2026-11-24):** juiz visual aprovou ("aprovado") na instância
`wayland-aero-f2-workspaces-visual`: 2 workspaces/2 `foot`, alternância pelas abas sem flicker.
Código/testes/gates verdes; ver `docs/STATE.md` §3. Instância deixada aberta para o humano. Cartão FECHADO.
