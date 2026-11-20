# STATE.md — estado atual do AeroShell (fonte única da verdade)
 
> Qualquer modelo/harness novo: leia `AGENTS.md`, este arquivo e `docs/RUNBOOK.md`, nesta ordem.
> ATUALIZE este arquivo ao fim de TODA sessão (seção "Handoff"). Mantenha-o curto (< 150 linhas):
> detalhes antigos vão para `docs/progress.md` (arquivo histórico).
 
Última atualização: 2026-11-19 · commit: `8f3e985` (na entrada desta sessão; ver `git log --oneline -3`) · modo: SPIKE
 
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
## 4. Em andamento
- [ ] **QUEBRADO** — frame por janela com `window_id` próprio. Código na branch `wip/window-id` (`f448dab`):
      só 1 janela decorada, mover às vezes move a outra, resize não acompanha. O teste `frame_window_id.py` existe
      nessa branch e NÃO foi rodado com sucesso. `master` (`bf44f22` + docs) é o estado bom: 1 janela decorada,
      resize acompanha com atraso/fundo branco no gap. Cartão: `docs/tasks/02b-window-id.md`.
## 5. Próximas tarefas (ordem; cada uma tem um cartão em docs/tasks/)
1. `02b-window-id.md` — 2 janelas decoradas, frame segue o resize; teste novo escrito primeiro.
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
## 7. Não medido / não validado (não trate como fato)
- Latência de resize com WebKit real; latência de ponteiro; qualquer desempenho em GPU real (tudo foi em VM sem 3D).
- Número de processos/PSS com `related-view` foi medido só em VM; ADR 0001 está "aceito provisoriamente".
- Estabilidade do `ext-background-effect` (blur): não implementado; Smithay 0.7 não tem handler.
- Nenhuma validação visual com juiz humano foi feita nestas duas últimas sessões (só documentação). Item visual só entra
  na seção 3 com o "ok" do humano registrado, conforme `AGENTS.md §0.9`.
## 8. Ambiente
Ubuntu 26.04 em VirtualBox (sem 3D), host pode ser X11. WebKitGTK 6.0 (2.52.x), gtk4-layer-shell, Rust via rustup, Bun.
WPE ausente no Ubuntu 26.04. Alvo de pacote: Debian 13 (trixie) + Ubuntu 26.04.
 
## 9. Handoff (reescreva a cada sessão, 10 linhas no máximo)
- O que eu estava fazendo: tarefa 00 — gate confiável; 3 gates verdes, diagnóstico do S1 e PIDs próprios.
- O que funcionou: falha induzida no S1 mostra stderr real; trap TERM/INT mata o filho pelo PID; nenhum `pkill -x` em `tests/`.
- Comando exato para reproduzir o estado atual: `git status --short && bash scripts/check.sh && (grep -rn "pkill -x" tests/ || echo NENHUM)`
- Próximo passo imediato (uma ação): abrir `docs/tasks/02b-window-id.md` — escrever `frame_follows_resize.py` PRIMEIRO e ver falhando na `wip/window-id`.
- Descobertas laterais registradas na seção 6 (não corrigidas): protocolos duplicados no `AGENTS.md`, título órfão, causa do flake do S1 desconhecida.
 