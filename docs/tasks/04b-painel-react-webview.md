# Tarefa 04b — Painel React na WebView: lista janelas e foca a clicada (executada; aguarda juiz visual)

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
No nested com 2 e depois 3 `foot`, o painel em React/TSX mostra id + título das janelas e um clique numa linha eleva somente aquela janela.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-shell/src/scheme.rs:5-9` — `PANEL_URI = aero://panel/index.html` e `PANEL_HTML` hoje é um stub estático
  ("CLIQUE AQUI (input)") usado nos testes de input region (S3).
- `crates/aeroshell-shell/src/bridge.rs:11-25` — handler `aero` só aplica input regions e faz log; **não encaminha comandos ao IPC**.
- `crates/aeroshell-compositor/src/window_ipc.rs` — `list` (`W id x y w h <titulo-json>`) e `focus <id>`.
- `packages/sdk/src/index.ts` — `createAero(transport)`, `parseWindows`, `aero.windows.list/focus`.
- `docs/architecture.md` — WebView é cliente Wayland; em produção só `aero://` é carregado.
- `docs/STATE.md:15` — objetivo da fatia vertical: painel React lista janelas e foca a clicada.

## Critério de aceite (comandos exatos + saída esperada)
1. `bun build packages/panel/src/index.tsx --target=browser --outfile=packages/panel/dist/app.js` → bundle gerado;
   `aero://panel/index.html` + `aero://panel/app.js` servem o painel (sem navegação externa, sem dev server em produção).
2. Pré-filtro `python3 tests/integration/react_panel.py` (escrito PRIMEIRO, visto falhando com "bundle ausente"):
   com 2 e 3 `foot`, o painel lista ids/títulos; um clique no 1º botão (a janela de trás) inverte a ordem de render
   no log do compositor; as outras janelas permanecem. Usar PIDs próprios, sem `pkill` genérico.
   Nota: o cartão previa a 2ª linha, mas ela já nasce na frente — clicar nela não mudaria a ordem e o teste daria
   falso negativo. O 1º botão é a janela de trás, então prova a elevação.
3. Teste manual (humano): abrir o nested do RUNBOOK com o painel e 2 `foot`, depois 3; verificar na tela que a lista
   acompanha abrir/fechar janela e que o clique eleva só a janela clicada. O agente deixa a aplicação **aberta** e pede
   o "ok"; quem fecha é o humano (AGENTS.md §0.9).
4. `bash scripts/check.sh` verde.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Hot reload, layouts, workspaces, estilização avançada, lock/greeter, 02d (resize), maximizar reposicionando cliente.

## Ponto de decisão ANTES de executar (mudança de arquitetura — AGENTS.md §5)
A WebView não tem Bun e não abre Unix socket. O caminho mínimo proposto é: o JS do painel faz
`window.webkit.messageHandlers.aero.postMessage({type:'focus', id})`; `bridge.rs` encaminha `focus <id>` ao socket IPC
usando a mesma API de `window_ipc.rs`. Isso **adiciona um tipo de mensagem ao bridge privado** (não é `protocols/*.xml`).
Alternativa: servir o painel pelo dev server Bun com WebSocket até o runtime — mais peças e fora do escopo desta fatia.
**Confirmar o caminho do bridge antes de começar.**

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md §6` (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem resultado visível → pare e descreva o bloqueio.
- Dependências novas: React 19.2.0 + react-dom 19.2.0 (MIT, já aprovadas). `bun build` já existe. Nada de npm extra.
- Arquivos protegidos: ver AGENTS.md §6 (`protocols/*.xml` NÃO muda).

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §7, §9) e comitar localmente (sem push).
