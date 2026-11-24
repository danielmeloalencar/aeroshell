# Tarefa F2-launcher — busca e inicialização de aplicativos

> Cartão de tarefa: um agente deve conseguir executá-lo lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
Buscar aplicativos instalados por fuzzy matching no SDK Bun e lançá-los pelo painel React no workspace ativo ou escolhido, focando a janela existente quando o app já estiver aberto.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-ipc/src/types.rs` — `App`, `AppListUpdated` e demais tipos públicos gerados para TS por ts-rs.
- `crates/aeroshell-compositor/src/window_ipc.rs` — stream `AeroEvent`, replay no subscribe e comandos IPC existentes.
- `crates/aeroshell-compositor/src/workspaces.rs` — workspace ativo e membership das janelas.
- `packages/sdk/src/` — API pública tipada e transporte IPC.
- `packages/panel/src/index.tsx` — painel React, abas de workspaces e lista de janelas.
- `tests/integration/f2_workspaces.py` e `f2_workspaces_panel.py` — vizinhos obrigatórios de workspaces/painel.
- ADRs relevantes: nenhum ADR aceito será alterado.

## Mecanismos
1. **Compositor:** ler arquivos `.desktop` de `/usr/share/applications` e `~/.local/share/applications`; publicar a lista pelo stream `AeroEvent` no evento `AppListUpdated`; aceitar `launch(app_id: string, workspace_id?: number)` pelo IPC.
2. **SDK Bun:** expor `listApps(): App[]`, `searchApps(query: string): App[]` e `launchApp(app_id: string, workspace_id?: number): void`; `App` contém `name`, `icon`, `exec` e `categories`; busca fuzzy cobre nome e categorias.
3. **Painel React:** botão/menu abre o launcher; campo de busca filtra em tempo real; lista mostra ícone e nome; clique lança no workspace ativo ou no especificado.
4. **Comportamento:** lançar no workspace ativo por padrão ou no workspace de destino informado; se o app já estiver aberto, focar a janela existente em vez de criar outra instância.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Baseline test-first:** `python3 tests/integration/f2_launcher.py` → `VEREDITO: FALHA` antes da implementação, por ausência do launcher/lista/comando; depois → `VEREDITO: OK`, cobrindo lista de apps, busca `ged` → `gedit`, busca inexistente → lista vazia, lançamento no workspace ativo/destino e foco sem segunda instância.
2. `bun test packages/sdk` → testes novos de fuzzy match, eventos e lançamento passam; o teste novo é visto falhando antes da implementação.
3. `python3 tests/integration/f2_workspaces.py && python3 tests/integration/f2_workspaces_panel.py` → ambos `VEREDITO: OK`.
4. Vizinhos de frame/CSD/policy/hot-reload relevantes continuam `VEREDITO: OK`; qualquer falha é preservada e comparada com o HEAD antes de atribuir regressão.
5. `bash scripts/check.sh` → `OK: scripts/check.sh passou`; `cargo clippy --workspace --all-targets --features bench -- -D warnings` → exit 0; `cargo check --workspace --no-default-features` → exit 0.
6. **Teste manual (juiz humano):** abrir o launcher → apps aparecem; digitar `ged` → `gedit` é filtrado; clicar → `gedit` abre no workspace ativo; repetir com workspace de destino → abre/é ativado no workspace correto; buscar app inexistente → lista vazia; lançar app já aberto → foca a janela existente sem abrir outra. O humano deve responder explicitamente **“ok/aprovado”**; a aplicação fica aberta para ele fechar.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Launcher como overlay fullscreen.
- Histórico de apps recentes.
- Snap/F7/F1.
- Mudanças em protocolos Wayland/XML, ADRs ou arquitetura.
- Novas dependências externas.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem resultado visível → pare e descreva o bloqueio.
- Dependências novas: não.
- Arquivos protegidos: ver AGENTS.md §6; nenhuma alteração prevista.
- Sem push; commits locais pequenos somente após validação.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §6, §7, §9), obter o veredito visual e comitar localmente (sem push).

## Veredito (2026-11-24)
Humano operou a instância `wayland-aero-f2-launcher-visual` com apps reais e respondeu **"ok aprovado"**.
Cartão fechado; aplicação deixada aberta para o humano.
