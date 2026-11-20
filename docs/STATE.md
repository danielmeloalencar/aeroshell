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
## 4. Em andamento
- Nenhuma. Próxima: `03-runtime-bun-sdk.md` (Aguardando sua autorização para iniciar).
## 5. Próximas tarefas (ordem; cada uma tem um cartão em docs/tasks/)
1. `03-runtime-bun-sdk.md` — `aero.windows.list/focus/close`, snapshot + patches; tipos TS gerados de `aeroshell-ipc`.
2. Passo 4 — Painel React (`useWindows()`), clique foca a janela certa; testar com 2 e 3 janelas (cartão a criar).
3. Decisão sobre modo produto (sua).
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
- `frame_resize_edges.py` é intermitente: 1 de 3 execuções nesta sessão falhou no `BottomRight` (`hit_no_frame=False`).
  Causa identificada no log cru `/tmp/edges-comp-25191.log`: o probe `Right` aplicou `content_w=1002`, mas o teste
  calcula `EDGE_POINTS` UMA vez com `696`; o ponto (759,577) passa a cair dentro do conteúdo. Não é regressão de
  `focus_on_press`. Amostra falha registrada, não descartada; corrigir o teste (recalcular pontos) é tarefa futura.
- 02b (dívida menor): `window_id` ainda é `space.elements().count()` no map; se uma janela fecha e outra abre, o id
  pode ser reatribuído. O mapa `window_surfaces` é limpo no hook de destruição, mas ids monotonicamente estáveis
  (não reutilizados) ficam para uma tarefa futura se o cenário aparecer.
## 7. Não medido / não validado (não trate como fato)
- Latência de resize com WebKit real; latência de ponteiro; qualquer desempenho em GPU real (tudo foi em VM sem 3D).
- Número de processos/PSS com `related-view` foi medido só em VM; ADR 0001 está "aceito provisoriamente".
- Estabilidade do `ext-background-effect` (blur): não implementado; Smithay 0.7 não tem handler.
- Nenhuma validação visual pendente: 02b foi validada pelo humano em 2026-11-20 (registro em §3). Demais itens visuais
  das seções anteriores seguem como histórico, não reabertos nesta sessão.
- 02b (honestidade metodológica): **resize**, **clique na barra da janela de trás** e **barra da frente sobre conteúdo**
  foram vistos FALHANDO antes das respectivas correções. A medição de **foco** foi adicionada depois do `nth` e não
  foi vista falhando antes; medida depois: foco em A, nunca em B. No 3º caso, B já estava focada: o critério medido
  é "o foco não salta para A", não "novo focus=true em B".
## 8. Ambiente
Ubuntu 26.04 em VirtualBox (sem 3D), host pode ser X11. WebKitGTK 6.0 (2.52.x), gtk4-layer-shell, Rust via rustup, Bun.
WPE ausente no Ubuntu 26.04. Alvo de pacote: Debian 13 (trixie) + Ubuntu 26.04.
 
## 9. Handoff (reescreva a cada sessão, 10 linhas no máximo)
- O que eu estava fazendo: tarefa 02b — FECHADA; `ok` do humano registrado e instância fechada pelo próprio humano.
- O que funcionou: 2 frames HTML, ações por `window_id`, clique na decoração eleva/foca a dona, barra da frente
  sobre conteúdo da de trás não "passa direto". Gate completo verde; 4 testes de integração OK.
- Comando exato: `python3 tests/integration/frame_follows_resize.py && python3 tests/integration/frame_window_id.py && bash scripts/check.sh`
- Próximo passo imediato (uma ação): abrir `docs/tasks/03-runtime-bun-sdk.md` e aguardar autorização para iniciar.
- Descobertas laterais em §6: `frame_resize_edges.py` intermitente (causa identificada), flake do S1 sem causa,
  `ConnectionRefusedError` no IPC do teste (corrigido), reatribuição de `window_id` após close.
 