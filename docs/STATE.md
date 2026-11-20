# STATE.md — estado atual do AeroShell (fonte única da verdade)
 
> Qualquer modelo/harness novo: leia `AGENTS.md`, este arquivo e `docs/RUNBOOK.md`, nesta ordem.
> ATUALIZE este arquivo ao fim de TODA sessão (seção "Handoff"). Mantenha-o curto (< 150 linhas):
> detalhes antigos vão para `docs/progress.md` (arquivo histórico).
 
Última atualização: 2026-11-20 · commit: `8f5a97b` (na entrada da tarefa 02b; ver `git log --oneline -3`) · modo: SPIKE
 
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
- [x] 02b (não regressão): `frame_buttons.py`, `frame_resize_edges.py`, `frame_follows_resize.py` → 3× OK.
      `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0).
## 4. Em andamento
- [ ] 02b — código pronto e gate verde; falta SOMENTE a confirmação visual do humano com 2 `foot` (critério 4 do
      cartão). Aplicação deve ser deixada aberta; quem fecha é o humano (`AGENTS.md §0.9`). Cartão:
      `docs/tasks/02b-window-id.md`.
## 5. Próximas tarefas (ordem; cada uma tem um cartão em docs/tasks/)
1. `02b-window-id.md` — validação visual final (2 janelas no nested, mover/resize/✕/foco), depois fechar tarefa.
2. `03-runtime-bun-sdk.md` — `aero.windows.list/focus/close`, snapshot + patches; tipos TS gerados de `aeroshell-ipc`.
3. Passo 4 — Painel React (`useWindows()`), clique foca a janela certa; testar com 2 e 3 janelas (cartão a criar).
4. Decisão sobre modo produto (sua).
## 6. Dívidas e bugs conhecidos (NÃO corrigir fora de uma tarefa; só registrar)
- Atraso da decoração no resize (frame repinta depois do conteúdo; VM sem 3D piora). Causas baratas aplicadas
  (frame usa geometria commitada; fundo transparente). Timer de 20 ms no frame_webview.rs ainda existe (loop de despacho).
  Alternativas se persistir: nove fatias no compositor ou buffer sobredimensionado + src-rect (ADR 0001, Plano B).
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
- 02b (dívida menor): `window_id` ainda é `space.elements().count()` no map; se uma janela fecha e outra abre, o id
  pode ser reatribuído. O mapa `window_surfaces` é limpo no hook de destruição, mas ids monotonicamente estáveis
  (não reutilizados) ficam para uma tarefa futura se o cenário aparecer.
## 7. Não medido / não validado (não trate como fato)
- Latência de resize com WebKit real; latência de ponteiro; qualquer desempenho em GPU real (tudo foi em VM sem 3D).
- Número de processos/PSS com `related-view` foi medido só em VM; ADR 0001 está "aceito provisoriamente".
- Estabilidade do `ext-background-effect` (blur): não implementado; Smithay 0.7 não tem handler.
- Nenhuma validação visual com juiz humano foi feita nestas duas últimas sessões (só documentação). Item visual só entra
  na seção 3 com o "ok" do humano registrado, conforme `AGENTS.md §0.9`.
- 02b: confirmação visual das 2 janelas PENDENTE — o agente deve deixar o nested aberto e o humano dar o "ok".
- 02b (honestidade metodológica): a parte de **foco** do `frame_follows_resize.py` foi adicionada DEPOIS da correção
  do `nth`; logo ela não foi vista falhando antes. A parte de resize foi vista falhando antes (A inalterada, B mudou).
  O foco foi medido depois: 1º clique → `wl_surface@3[1]` (A); 2º clique em A → nenhum `focus=true` para B.
## 8. Ambiente
Ubuntu 26.04 em VirtualBox (sem 3D), host pode ser X11. WebKitGTK 6.0 (2.52.x), gtk4-layer-shell, Rust via rustup, Bun.
WPE ausente no Ubuntu 26.04. Alvo de pacote: Debian 13 (trixie) + Ubuntu 26.04.
 
## 9. Handoff (reescreva a cada sessão, 10 linhas no máximo)
- O que eu estava fazendo: tarefa 02b — code completa, gate verde, 4 testes de integração OK.
- O que funcionou: `frame_requested` para 2 janelas, WebView por window_id, resolução estável (`window_for_id`).
  ANTES/DEPOIS do resize provados com o mesmo comando; foco por clique medido (nunca em B).
- Comando exato: `python3 tests/integration/frame_follows_resize.py && python3 tests/integration/frame_window_id.py && bash scripts/check.sh`
- Próximo passo imediato (uma ação): abrir o nested com 2 `foot`, deixar aberto, pedir o "ok" visual ao humano (critério 4).
- Descobertas laterais em §6: protocolos duplicados no `AGENTS.md`, flake do S1 sem causa, reatribuição de `window_id`.
 