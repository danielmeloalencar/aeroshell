# STATE.md — estado atual do AeroShell (fonte única da verdade)
 
> Qualquer modelo/harness novo: leia `AGENTS.md`, este arquivo e `docs/RUNBOOK.md`, nesta ordem.
> ATUALIZE este arquivo ao fim de TODA sessão (seção "Handoff"). Mantenha-o curto (< 150 linhas):
> detalhes antigos vão para `docs/progress.md` (arquivo histórico).
 
Última atualização: 2026-11-20 · tarefa ativa: nenhuma (04b fechada com ok visual) · modo: SPIKE
 
## 1. Em uma frase
Framework para criar compositores Wayland (Rust + Smithay) cuja UI inteira (painéis, frames de janela,
lock, login) é TSX renderizado por WebKitGTK; runtime de política em Bun. Dev em nested (winit).
 
## 2. Objetivo atual (marco da "fatia vertical")
`foot` abre no AeroShell nested com barra de título em HTML que se arrasta/redimensiona/fecha/maximiza,
e um painel em React/TSX lista as janelas e foca a clicada. Quando isso funcionar, decidir sobre o modo produto.
 
## 3. Feito (com onde está a prova)
- [x] Compositor nested (winit) com socket próprio, xdg-shell, layer-shell — `scripts/check.sh` (S1 nested)
- [x] Painel WebKitGTK transparente como layer-shell, input regions do DOM, passagem de clique (S2/S3, validado à mão)
- [x] `aero_frame_v1` v1 (opção B: `associate` só MARCA a superfície, não atribui role; ver ADR 0004) — `protocol_tests` (17 testes)
- [x] Supervisor dentro do compositor, socketpair + `WAYLAND_SOCKET`, PDEATHSIG via rustix (ADR 0005) — `tests/integration/supervisor_lifecycle.py`
- [x] Frame em WebView com mover e redimensionar (8 arestas/cantos) — `tests/integration/frame_resize_edges.py`
- [x] `xdg-decoration` server-side (o `foot` abre mão da própria barra)
- [x] Botões maximizar/restaurar/fechar — `tests/integration/frame_buttons.py`
- [x] Regra de validação visual com juiz humano (agente executa/deixa aberto, humano testa e fecha) — `AGENTS.md §0.9`,
      `§0.7` e checklist `§9`; prova: `git diff -- AGENTS.md`
- [x] `docs/progress.md` despromovido a histórico — `AGENTS.md:3,154,262,272`, `plan.md`, `README.md`, `justfile`
      e ponteiros em `docs/plano-mestre.md` (plano ATIVO, preservado); prova: `grep -rn "progress" AGENTS.md plan.md README.md justfile`
- [x] `AGENTS_SESSAO_ADENDO.md` removido a pedido do usuário (protocolo de sessão já vive no próprio `AGENTS.md`).
- [x] Tarefa 00 — gate confiável: 3× `bash scripts/check.sh` verdes seguidas (bun 4,39/4,41/4,42 s). Falha induzida
      no S1 (`XDG_RUNTIME_DIR` inexistente) passou a mostrar `Error: PermissionDenied` + log do compositor;
      `finally` e trap TERM/INT encerram só o PID iniciado (provado: filho 13317 morreu ao matar o pai 13302);
      `grep -rn "pkill -x" tests/` vazio. Causa do flake antigo NÃO identificada (não reproduziu).
- [x] 02b (código): 2 `foot` com frame HTML cada; ✕/maximizar roteados por `window_id`. Prova:
      `python3 tests/integration/frame_window_id.py` → OK (`associate: [0, 1]`, `close 1`, janela 0 permanece).
- [x] 02b (causa raiz): `window_id` é lógico/estável, mas 6 resolvers usavam `space.elements().nth(id)` (ordem z,
      reordenada por `raise_element` no clique). Mapa estável `window_surfaces` + `window_for_id()` em `state.rs`.
      Prova: `tests/integration/frame_follows_resize.py` ANTES: A inalterada / B mudou (`100→60`, `696→732`);
      DEPOIS: A mudou (`60→20`, `696→732`) / B idêntica — mesmo comando, 1 execução cada.
- [x] 02b (não regressão): `frame_buttons.py`, `frame_resize_edges.py`, `frame_follows_resize.py` → OK.
      `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0).
- [x] 02b (clique na decoração, relato do humano): `focus_on_press` agora cai no frame via `surface_under` +
      `associated_frames` quando `element_under` não vê janela (titlebar é layer surface). Prova:
      `frame_follows_resize.py` ANTES: `ordem ... BARRA` inalterada, `foco=[]`, `barra eleva A=False`;
      DEPOIS: ordem inverte, `foco=['wl_surface@3[1]']` (A), `barra eleva A=True`.
- [x] 02b (barra da frente sobre conteúdo da de trás, relato do humano): DUAS causas corrigidas:
      (1) `surface_under` verificava o CLIENTE antes da decoração dos frames — o conteúdo de A roubava o hit da barra
      de B; agora a decoração vem primeiro e percorre o Space do topo (z-order) para baixo;
      (2) `focus_on_press` usava `element_under` (ignora layers) — trocado por `surface_under` como fonte única,
      resolvendo frame→`window_id` ou cliente→`Window`.
      Prova: ANTES `superfície em (200,100)=wl_surface@3[1]` (conteúdo de A), `frente antes y=120, depois y=80`,
      `barra da frente não passa=False`; DEPOIS `superfície=wl_surface@45[0]` (frame de B), `frente 120→120`,
      `foco=[]` (B já estava focada, foco não saltou para A), `barra da frente não passa=True`.
- [x] Gate final desta rodada: `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0);
      `frame_follows_resize.py`, `frame_window_id.py`, `frame_buttons.py`, `frame_resize_edges.py` → 4× OK.
- [x] 02b — VALIDAÇÃO VISUAL DO HUMANO (2026-11-20): "ok validado e fechada a instancia", após testar na janela
      aberta com 2 `foot` o clique na barra da janela da frente sobre o conteúdo da de trás; o próprio humano fechou
      a aplicação (`AGENTS.md §0.9`). Tarefa 02b FECHADA.
- [~] 02c (PARCIAL): instrumentação `RESIZE_SYNC_{MOTION,CLIENT,FRAME}` sob `cfg(feature="bench")` em `grab.rs`,
      `compositor.rs`; teste `tests/integration/frame_resize_sync.py` escrito e visto FALHANDO antes da correção
      (14/14, depois 13/14 passos pendentes). Correção mantida: `send_frame_configure` no próprio `ResizeGrab::motion`
      (`grab.rs:245`), em vez de só no commit do cliente. Prova com a MESMA métrica, reprocessando os logs crus:
      baseline `lag p95=177,22 ms / 13-14 pendentes`; após a correção `127,19 ms / 13-14` (−28% no lag, −1 commit).
      Critério 1 do cartão NÃO passa (falta `0` pendentes); gate `bash scripts/check.sh` verde.
- [~] 02c (VEREDITO VISUAL DO HUMANO, 2026-11-20): com a correção parcial já aplicada, arrastou a borda em ritmos
      lento e rápido e disse: **"ainda está do mesmo jeito"**. Ou seja, o ganho de 28% no lag não foi percebido na
      tela; o sintoma "decoração correndo atrás" PERSISTE. Humano pediu manter para outra tarefa e seguir em frente —
      continuação no cartão `02d-frame-resize-diagnostico.md`. Instância fechada pelo próprio humano.
- [x] 03 (tipos): `ts-rs 12.0.1` (MIT, aprovado) gera 4 bindings em `packages/sdk/src/generated/` a partir de
  `crates/aeroshell-ipc/src/types.rs`. Prova: `bun run generate:types` → `4 passed` e arquivos `WindowInfo/Snapshot/
  WindowChange/Patch.ts` com o cabeçalho "generated by ts-rs".
- [x] 03 (critério 1): `bun test packages/sdk` → `5 pass / 0 fail / 16 expect()`; store aplica snapshot, aplica
  patch Upsert/Remove e ignora revisão antiga/igual (identidade do objeto preservada, zero notificações).
- [x] 03 (IPC): `window_ipc.rs` agora lista `W id x y w h <titulo-json>` com `id` do mapa lógico `window_surfaces`
  (não `enumerate()` da ordem z) e título lido de `XdgToplevelSurfaceData.title` via `with_states`.
- [x] 03 (critérios 2/3, pré-filtro): `python3 tests/integration/sdk_cli.py` → `--list ['0\tfoot', '1\tfoot']`;
  `--focus 0` inverteu a ordem de render de `[...,@3[1],...,@3[0]]` para `[...,@3[0],...,@3[1]]`;
  `--close 1` deixou só a janela 0. Amostra crua: `/tmp/opencode/sdk_cli_run5.log`.
- [x] 03 (gate): `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0), `bun test` 6 pass / 0 fail.
- [x] 03 — VALIDAÇÃO VISUAL DO HUMANO (2026-11-20): com o nested aberto (2 `foot` + shell de frame), o humano rodou
  `--list` (2 linhas com id/título), `--focus 0` (elevou a janela certa) e `--close 1` (fechou só aquela) e respondeu
  **"ok validado"**. O próprio humano fechou a aplicação (PID 39214 encerrado); nenhum processo meu ficou órfão.
  Tarefa 03 FECHADA.
- [x] 04a (critério 1): `bun test packages/react` → `3 pass / 0 fail / 8 expect()`. O teste foi escrito antes e visto
  falhando (`Cannot find module './useWindows'`), depois passou. `useWindows` liga `store.subscribe`/
  `store.getSnapshot` ao `useSyncExternalStore`; patch notifica e unsubscribe desliga o listener.
- [x] 04a (deps): React 19.2.0 + react-dom 19.2.0 (MIT, aprovados) instalados no workspace `packages/react`;
  `bun.lock` versionado. Prova: `bun -e 'import("react")'` dentro de `packages/react` → `react real ok function`.
- [x] 04a (critérios 2/3): `bun run generate:types` → `EXIT=0` e `git diff -- packages/sdk/src/generated` vazio;
  `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0), `bun test` 9 pass / 0 fail.
- [x] 04b (pré-filtro): `python3 tests/integration/react_panel.py` → `OK: painel React listou 2 e 3 janelas;
  clique no botão da janela de trás elevou só ela; bridge continuou respondendo`. Escrito antes e visto falhando
  (`bundle ausente`, depois `bridge parou de responder após o clique`). O teste prova: bundle servido por
  `aero://panel/app.js`, lista `(id,título)` entregue via bridge, clique injetado no botão escolhido pela ordem de
  render inverte a ordem sem alterar as demais.
- [x] 04b (travamento do bridge): causa raiz reproduzida e corrigida. `forward()` esperava o `.` que só o `list`
  envia; `focus` responde `OK` e o main loop congelava (botões na tela, porém mortos). Teste de regressão escrito
  antes e visto falhando: `FALHA: bridge parou de responder após o clique`. Com `shutdown(Shutdown::Write)`:
  `bridge continuou respondendo` com 2 e 3 janelas. Commit `df89000`.
- [x] 04b (gate final): `bash scripts/check.sh` → `OK: scripts/check.sh passou`, `EXIT=0`, `bun test 9 pass / 0 fail`.
- [x] 04b (juiz visual — 2026-11-20): humano: **"tudo foi ok agora, inclusive abrir outro foot via comando em um
  dos foot"**. Prova no log `/tmp/opencode/aero-04b-fix.log`: após o 4º `foot` (`toplevel mapeado ...
  wl_surface@3[4]`), o painel listou `0 "foot" | 1 "foot" | 2 "foot" | 3 "foot"` e publicou 4 rects. Humano fechou
  a aplicação. **04b FECHADA.**
## 4. Em andamento
- Nenhuma tarefa em andamento: 04b fechada com ok visual; 02c segue bloqueada (decisão pendente do humano).
- 02c BLOQUEADA: duas abordagens tentadas e insuficientes no critério de pendência — (1) configure no motion e
  (2) timer de despacho do shell de 20→1 ms (revertido: p95 piorou 127→158 ms). Terceira abordagem exige decisão
  do humano (`AGENTS.md §3.12`, regra das duas tentativas). Não abrir validação visual até a decisão.
## 5. Próximas tarefas (ordem; cada uma tem um cartão em docs/tasks/)
1. `05-compatibilidade-protocolos.md` — RASCUNHO, aguarda decisão do humano. 7 mensagens do `foot` registradas em
   §6 como requisito do projeto; levantar na versão fixada do Smithay antes de implementar. NÃO executar sem ok.
2. `02d-frame-resize-diagnostico.md` — RASCUNHO criado, aguarda o "ok" do humano. Medir 5 cadências × 3
   execuções + juiz visual para decidir entre Plano B e redefinição de critério (decisão do humano em 2026-11-20:
   "Manter parcial, cartão novo").
3. Decisão sobre modo produto (sua) — o juiz visual da fatia vertical (04b) já foi obtido em 2026-11-20.
4. (Opcional) corrigir S3: hoje falha no baseline por input region de layer surface — ver §6.
## 6. Dívidas e bugs conhecidos (NÃO corrigir fora de uma tarefa; só registrar)
- 04b: `tests/integration/s3_input.sh` falha **também no baseline** (`0fc05e7`, worktree isolado): a input region
  é aplicada (`rects=[(24,8,189,36)]`) mas o clique "dentro" dá `under=None`. Não é regressão do painel React;
  S3 não está no gate. 04b não mexeu no teste (tentei `AERO_PANEL_LEGACY=1`, sem efeito, e revertei).
- **REQUISITO DO PROJETO (decisão do humano, 2026-11-20): compatibilidade com compositores modernos.** O humano
  relatou na tela estas mensagens do `foot` e exigiu que todas sejam implementadas no AeroShell (ou registradas
  como requisito se não for o momento): `compositor does not implement the primary selection interface`;
  `compositor does not implement xdg activation`; `bell.urgent will fall back to coloring the window margins red`;
  `compositor does not implement fractional scaling`; `compositor does not implement server-side cursors, falling
  back to client-side cursors`; `compositor does not implement the xdg toplevel icon protocol`;
  `text interface not implemented compositor, IME will be disabled`. Nomes de protocolo prováveis (CONFIRMAR na
  versão fixada de Smithay/foot antes de implementar): wp-primary-selection-v1, xdg-activation-v1,
  wp-fractional-scale-v1, wp-cursor-shape-v1, xdg-toplevel-icon-v1, zwp-text_input_v3/IME, além de urgência/atenção
  de toplevel. Não implementados nesta fatia (exige ADR + cartões novos); rascunho em `docs/tasks/05-*.md`.
- 03: o socket IPC é síncrico (request/response); não há eventos de map/unmap. Os patches do SDK são derivados da
  resposta `list` do compositor no `createAero.refresh()`. Streaming de eventos + codec MessagePack é o S4 do plano
  mestre (exige ADR e dependências novas) — NÃO foi feito aqui.
- 03: `send_close()` é assíncrono. A 1ª versão de `sdk_cli.py` checou o `--list` imediatamente após o `OK` e falhou
  (`2 janelas` ainda). Amostra crua em `/tmp/opencode/sdk_cli_run.log`, não descartada; o teste agora espera o unmap
  por até 10 s. Não é bug do CLI.
- Atraso da decoração no resize (frame repinta depois do conteúdo; VM sem 3D piora). Causas baratas aplicadas
  (frame usa geometria commitada; fundo transparente). Timer de 20 ms no frame_webview.rs ainda existe (loop de despacho).
  Alternativas se persistir: nove fatias no compositor ou buffer sobredimensionado + src-rect (ADR 0001, Plano B).
- PRIORIDADE ALTA (relato do humano, 2026-11-20, logo após fechar 02b): "o conteúdo acompanha o mouse muito bem,
  porém a decoração fica atrasada correndo atrás". É o mesmo item acima, agora com sentença do juiz humano —
  NÃO é só dívida teórica. Cartão rascunho: `docs/tasks/02c-frame-resize-sync.md` (aguarda confirmação para executar).
  Leitura preliminar (HIPÓTESE, não medida): o frame só é reconfigurado no commit do cliente
  (`protocols/compositor.rs:130-149`) e o shell despacha eventos Wayland num timer de 20 ms
  (`frame_webview.rs:458`), somando um ciclo entre conteúdo e decoração.
- Maximizar não move o cliente para a origem do output; restaurar manda `size = None`.
- `button: 0x110` fixo no grab de resize (só botão esquerdo).
- `--command` via `sh -c` pode orfanar netos (usar exec/lançamento direto).
- `pkill -x aeroshell-compositor` NÃO casa (comm é truncado a 15 chars: `aeroshell-compo`); harnesses que dependem disso
  não matam instâncias antigas. Usar `pkill -f` com padrão específico ou rastrear PIDs.
- Presentation-time não existe; socket órfão; lock/greeter/PAM/idle/XWayland: não iniciados.
- Probe `wayland-client` no shell deve ficar atrás da feature `probe-registry` (optional).
- `AGENTS.md` tem dois protocolos sobrepostos: novo `§0.1–0.9` (linhas 7–106) e antigo "Início/Foco/Fim" (linhas ~108–131).
  Conflito: linha 16 manda corrigir o gate se falhar; antiga linha ~94 manda parar. Há também um título órfão no fim.
  As referências a `progress.md` como fonte única JÁ foram corrigidas (tarefa 05); estes itens seguem pendentes.
- Flake do S1: falhou 1× no passado (exit 1, ~16 s sob carga; isolado 7,67 s). Na tarefa 00 NÃO reproduziu:
  3 gates seguidos verdes, nenhuma amostra descartada (tempos em §3). Diagnóstico melhorado (stderr + PID),
  causa ainda DESCONHECIDA — não declarar "corrigido". Reexecutar até passar continua proibido (§0.5).
- Erro meu corrigido nesta sessão: eu havia rebaixado `docs/plano-mestre.md` a histórico. O usuário corrigiu: é o
  plano ATIVO e um dos documentos mais importantes (visão, fases, onde queremos chegar). Banner removido, texto
  original restaurado; só os ponteiros de atualização de `progress.md` viraram `STATE.md`. Não repetir.
- 02b (relato do humano, 2026-11-19/20, 2 foot): 2 janelas deslocadas, só UMA decorada; arrastar/resize move a outra;
  ✕ e maximizar não faziam nada; clicar na janela focada focalava a outra. DUAS causas, ambas corrigidas nesta sessão:
  (1) o map não anunciava `frame_requested` para a 2ª janela; (2) `nth(window_id)` usava ordem z. Confirmado por log.
- 02b (relato do humano, 2026-11-20, após "ok" parcial): clicar na BARRA/DECORAÇÃO da janela de trás não a elevava
  (só com arrasto). Confirmado e CORRIGIDO: `element_under` ignora layer surfaces; fallback por `surface_under` +
  `associated_frames` em `input.rs`.
- 02b (relato do humano, 2026-11-20, 3º detalhe): barra da janela da FRENTE sobre o CONTEÚDO da de trás — o clique
  "passava direto" e elevava a de trás. Confirmado e CORRIGIDO (duas camadas: ordem em `surface_under` e fonte em
  `focus_on_press`). Amostra falha crua em `/tmp/opencode/overlap-before2.log`, não descartada.
- Amostra falha de infraestrutura do teste (registrada, não descartada): `frame_follows_resize.py` falhou 1× com
  `ConnectionRefusedError` no socket IPC antes da inicialização; `ipc()` agora espera até 15 s pelo socket.
  Também houve 1 execução com falha apenas na asserção de foco (B já estava focada, então não emite `focus=true`);
  a asserção foi corrigida para "foco não salta para A", que é o critério real.
- 02c (MEDIÇÃO, não é bug do produto): o `foot` arredonda o tamanho pedido para múltiplos de 6 (`want=706` →
  conteúdo real `702`). A primeira versão do teste comparava o frame contra o `want` do motion; a métrica foi
  corrigida em `frame_resize_sync.py` para usar o tamanho REAL commitado pelo cliente. Amostra falha crua em
  `/tmp/opencode/sync-before.log` e `/tmp/opencode/sync-before2.log`, não descartadas.
- 02c (LIMITAÇÃO ARQUITETURAL): com a decoração em WebView, o commit do frame leva ~60–100 ms após o configure;
  motions de 50 ms tornam "zero pendentes no próximo motion" inatingível sem tirar o WebKit do caminho crítico
  (Plano B, ADR 0001). Duas abordagens falharam; NÃO tentar uma terceira sem aprovação (`AGENTS.md §3.12`).
- 02c (timer 1 ms sem ganho): `frame_webview.rs` foi temporariamente trocado de 20 ms para 1 ms; p95 piorou
  (127,19 → 158,05 ms) e a pendência ficou em 13/14. Revertido para 20 ms. Amostra em `/tmp/opencode/sync-after3.log`.
- `frame_resize_edges.py` é intermitente: 1 de 3 execuções nesta sessão falhou no `BottomRight` (`hit_no_frame=False`).
  Causa identificada no log cru `/tmp/edges-comp-25191.log`: o probe `Right` aplicou `content_w=1002`, mas o teste
  calcula `EDGE_POINTS` UMA vez com `696`; o ponto (759,577) passa a cair dentro do conteúdo. Não é regressão de
  `focus_on_press`. Amostra falha registrada, não descartada; corrigir o teste (recalcular pontos) é tarefa futura.
- 02b (dívida menor): `window_id` ainda é `space.elements().count()` no map; se uma janela fecha e outra abre, o id
  pode ser reatribuído. O mapa `window_surfaces` é limpo no hook de destruição, mas ids monotonicamente estáveis
  (não reutilizados) ficam para uma tarefa futura se o cenário aparecer.
## 7. Não medido / não validado (não trate como fato)
- 04b: nada pendente — veredito visual POSITIVO do humano e causa do travamento registrados em §3. Instância da
  validação fechada pelo próprio humano; nenhum PID meu ficou órfão.
- 03: latência do IPC, taxa de perda e reconexão NÃO medidos (S4 do plano mestre, fora do cartão).
- Latência de resize com WebKit real; latência de ponteiro; qualquer desempenho em GPU real (tudo foi em VM sem 3D).
- Número de processos/PSS com `related-view` foi medido só em VM; ADR 0001 está "aceito provisoriamente".
- Estabilidade do `ext-background-effect` (blur): não implementado; Smithay 0.7 não tem handler.
- 02c: validação visual REALIZADA COM RESULTADO NEGATIVO (2026-11-20): humano arrastou a borda (lento e rápido) na
  instância com 2 `foot` e disse "ainda está do mesmo jeito". NÃO é "validado"; item segue em aberto em 02d.
  Instância fechada pelo próprio humano; log `/tmp/opencode/aero-vis5.log`.
- 02c: desempenho em GPU real (VM sem 3D); 3 execuções seguidas com a LOW final (a métrica foi reprocessada de 3
  execuções históricas, uma por variante — não são 3 repetições da mesma variante).
- Nenhuma validação visual pendente de 02b: validada pelo humano em 2026-11-20 (registro em §3). Demais itens visuais
  das seções anteriores seguem como histórico, não reabertos nesta sessão.
- 02b (honestidade metodológica): **resize**, **clique na barra da janela de trás** e **barra da frente sobre conteúdo**
  foram vistos FALHANDO antes das respectivas correções. A medição de **foco** foi adicionada depois do `nth` e não
  foi vista falhando antes; medida depois: foco em A, nunca em B. No 3º caso, B já estava focada: o critério medido
  é "o foco não salta para A", não "novo focus=true em B".
## 8. Ambiente
Ubuntu 26.04 em VirtualBox (sem 3D), host pode ser X11. WebKitGTK 6.0 (2.52.x), gtk4-layer-shell, Rust via rustup, Bun.
WPE ausente no Ubuntu 26.04. Alvo de pacote: Debian 13 (trixie) + Ubuntu 26.04.
 
## 9. Handoff (reescreva a cada sessão, 10 linhas no máximo)
- O que eu estava fazendo: fechamento de 04b após o ok visual do humano ("tudo foi ok agora, inclusive abrir outro
  foot via comando em um dos foot"); a instância foi fechada pelo humano.
- O que funcionou: `python3 tests/integration/react_panel.py` → `OK: painel React listou 2 e 3 janelas; clique no
  botão da janela de trás elevou só ela; bridge continuou respondendo`; log `/tmp/opencode/aero-04b-fix.log` com
  4 `foot` e 4 botões; `bash scripts/check.sh` green (EXIT=0, bun 9/9).
- Comando exato para reproduzir: `python3 tests/integration/react_panel.py` (sobe/encerra os próprios PIDs).
- Próximo passo imediato (uma ação): perguntar ao humano qual cartão entra na próxima sessão — 05 (compatibilidade
  Wayland, 7 mensagens do `foot`) ou 02d (diagnóstico do resize). NÃO executar nenhum dos dois sem ok.
- NÃO executar 02d (resize) nem tentar terceira abordagem sem autorização explícita.
- Descobertas laterais em §6: IPC sem eventos (patches vêm do `list`), `send_close` assíncrono (teste com polling),
  arredondamento do foot, timer 1 ms sem ganho, `frame_resize_edges.py` intermitente, flake do S1.
 