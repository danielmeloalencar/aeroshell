# Tarefa 03 — Runtime Bun + SDK mínimo

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
`aero.windows.list()`, `.focus(id)` e `.close(id)` funcionam a partir de um script TypeScript rodando em Bun,
com estado reativo (snapshot + patches) vindo do compositor pelo IPC existente.

## Contexto mínimo
- `crates/aeroshell-ipc/src/{messages,types}.rs` — mensagens; tipos TS DEVEM ser gerados daqui (ts-rs/specta), nunca à mão
- `crates/aeroshell-compositor/src/window_ipc.rs` — comandos já existentes (close/maximize)
- `packages/sdk/`, `packages/runtime/` — a criar

## Critério de aceite (comandos exatos + saída esperada)
1. `bun test packages/sdk` → testes do store (aplica snapshot, aplica patch, ignora patch com revisão antiga) passam.
2. Com o nested e 2 `foot` abertos: `bun run packages/runtime/src/main.ts --list` imprime 2 janelas com id e título.
3. `bun run packages/runtime/src/main.ts --close <id>` fecha só aquela janela.
4. Confirmação visual do humano: o agente deixa o nested aberto e pede o "ok" (AGENTS.md §0.9); quem fecha é o humano.
5. `bash scripts/check.sh` verde.

## Fora de escopo
Painel React (tarefa 04), hot reload, workspaces, layouts, estilização.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem resultado visível → pare e descreva o bloqueio
- Dependências aprovadas: nenhuma npm além do que já existe. (React/react-dom 19.2.0 exatos só na tarefa 04.)
- Arquivos protegidos: ver AGENTS.md §6

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §9) e comitar localmente (sem push).
