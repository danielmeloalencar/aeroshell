# Tarefa 04a — Adaptador React `useWindows()` (RASCUNHO — aguarda "ok")

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
`useWindows()` de `@aeroshell/react` devolve as janelas do store do SDK e acompanha snapshot/patch, provado por teste Bun sem WebView.

## Contexto mínimo (arquivos que importam)
- `packages/sdk/src/index.ts:1-70` — `createAero` devolve `{ store, windows }`; `WindowStore` em `store.ts` tem `subscribe()/getSnapshot()`.
- `packages/sdk/src/generated/*.ts` — tipos gerados por ts-rs (usar `import type`, nunca redeclarar).
- `docs/plano-mestre.md:57` — "SDK expõe stores reativos; adapters via `useSyncExternalStore` (React)".
- `packages/react/` — hoje só `.gitkeep`; `package.json` já tem workspaces `packages/*`.

## Critério de aceite (comandos exatos + saída esperada)
1. `bun test packages/react` → 3 testes passam:
   - o hook chama `useSyncExternalStore` com `store.subscribe` e `store.getSnapshot`;
   - ao aplicar `store.applyPatch({revision:2, change:{Upsert:...}})`, o valor observado muda para a nova lista;
   - `unsubscribe` é chamado ao desmontar.
   - Teste sem DOM: usar `mock.module("react", …)` do Bun para capturar os argumentos; NÃO adicionar happy-dom/jsdom.
2. `bun run generate:types` continua gerando os 4 arquivos em `packages/sdk/src/generated/` sem diffs manuais.
3. `bash scripts/check.sh` verde (6+ testes Bun, fmt/clippy/cargo test).
4. Sem critério visual neste cartão (lógica pura) — não precisa de "ok" do humano.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Painel na WebView, bundle TSX, bridge `postMessage` (tarefa 04b).
- HMR, layouts, workspaces, estilização, 02d (resize).

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem resultado visível → pare e descreva o bloqueio.
- Dependências novas: **React 19.2.0 e react-dom 19.2.0** (exatos, MIT, já aprovados em `docs/tasks/TEMPLATE.md:51`).
  Motivo: o adapter é React e o plano mestre exige `useSyncExternalStore`. Alternativa descartada: reimplementar reatividade sem React (não atende ao contrato).
- Arquivos protegidos: ver AGENTS.md §6.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §9) e comitar localmente (sem push).
