# Tarefa 02b — Um frame decorado por janela, cada um com o próprio window_id

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
Com 2 `foot` abertos, AMBOS têm barra HTML; mover, redimensionar e fechar atuam sempre na janela dona do frame clicado.

## Contexto mínimo (arquivos que importam)
- Ponto de partida (decisão do usuário em 2026-11-19): a branch `wip/window-id` (`f448dab`, QUEBRADO: só 1 frame
  decorado, mover às vezes move a outra, resize não acompanha) **FOI DESCARTADA como base**. Reimplementar no
  `master` (que já tem 1 janela decorada estável + docs + gate confiável). As tasks relacionadas ao wip devem ser
  reexecutadas agora ou depois; o código antigo serve só como referência do que NÃO fazer (`git show f448dab:...`).
- `crates/aeroshell-compositor/src/protocols/xdg_shell.rs` — anúncio de `frame_requested` no map
- `crates/aeroshell-shell/examples/frame_webview.rs` — uma WebView por window_id; handler captura o próprio id
- `crates/aeroshell-compositor/src/protocols/aero_frame.rs`, `grab.rs` — o alvo de move/close deve vir do frame clicado
- `tests/integration/frame_window_id.py` — existe na branch wip; mede 2 janelas/associates

## Critério de aceite (comandos exatos + resultado esperado)
1. `python3 tests/integration/frame_window_id.py` → OK (2 janelas, `frame_requested`/`associate` para os 2 ids).
   O teste existe só no wip (`f448dab`); trazer para o master e adaptar a matar só PID próprio (tarefa 00).
2. `tests/integration/frame_follows_resize.py` — escrever PRIMEIRO e ver falhando no master com 2 janelas: após um
   resize do cliente, o frame fica com (conteúdo + insets) em até 300 ms, para 1 e 2 janelas. Com 1 janela no master
   pode passar (o bug é de múltiplas janelas); registre o resultado com honestidade.
3. Não regredir: `python3 tests/integration/frame_resize_edges.py` e `python3 tests/integration/frame_buttons.py` → OK.
4. Teste manual (humano): 2 janelas decoradas; mover/redimensionar/fechar cada uma afeta só ela; o frame acompanha o resize.
   O agente executa, deixa a aplicação aberta e pede o "ok" — quem fecha é o humano (AGENTS.md §0.9).
5. `bash scripts/check.sh` verde COMPLETO (inclui fmt --check). Proibido pular etapas ou editar o gate.

## Método (obrigatório)
Antes de mexer no código: adicione logs com `window_id` nos 3 pontos da cadeia (compositor envia `frame_requested`;
shell recebe e cria WebView; compositor recebe `associate`) e rode com 2 janelas. Cole as linhas cruas e diga onde a cadeia quebra.
Corrija UM ponto por vez e rode o critério 3 a cada correção. Se a mesma abordagem falhar 2 vezes, pare e descreva.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Latência do resize, fundo branco no gap, maximizar reposicionando o cliente
- Pool de WebViews, painel React, SDK

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem resultado visível → pare e descreva o bloqueio
- Dependências novas: nenhuma sem aprovação (AGENTS.md §3.10)
- Arquivos protegidos: ver AGENTS.md §6 — NENHUMA mudança em `protocols/*.xml`

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §9) e comitar localmente (sem push).

## Resultado (2026-11-20)
- Critério 1: `python3 tests/integration/frame_window_id.py` → OK (`frame_requested`/`associate` para 0 e 1; `close 1`; janela 0 permanece).
- Critério 2: `frame_follows_resize.py` escrito antes da correção; ANTES falhou (A inalterada, B mudou) e DEPOIS passou
  (A mudou, B idêntica). Medição de foco por clique adicionada depois: foco nunca em B.
- Critério 3: `frame_resize_edges.py` e `frame_buttons.py` → OK (nenhuma regressão).
- Critério 5: `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0, gate completo).
- Critério 4 (visual): PENDENTE — requer o "ok" do humano com a aplicação aberta.
- Causa raiz corrigida: `nth(window_id)` (ordem z) trocado por `window_for_id()` (mapa estável) em `grab.rs`,
  `window_ipc.rs`, `aero_frame.rs` e `xdg_shell.rs`; `frame_webview.rs` cria uma WebView por window_id.
