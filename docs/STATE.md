# STATE.md — estado atual do AeroShell (fonte única da verdade)
 
> Qualquer modelo/harness novo: leia `AGENTS.md`, este arquivo e `docs/RUNBOOK.md`, nesta ordem.
> ATUALIZE este arquivo ao fim de TODA sessão (seção "Handoff"). Mantenha-o curto (< 150 linhas):
> detalhes antigos vão para `docs/progress.md` (arquivo histórico).
 
Última atualização: 2026-11-19 · commit: <PREENCHER `git rev-parse --short HEAD`> · modo: SPIKE
 
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
## 4. Em andamento
- [ ] Frame por janela com `window_id` próprio (antes usava `entries.first()`: ✕ da janela B fechava a A).
      Implementado (protocols/xdg_shell.rs anuncia `frame_requested`; frame_webview.rs cria 1 WebView por window_id).
      FALTA: rodar `tests/integration/frame_window_id.py` e ver OK. Verifique antes de assumir que passa.
## 5. Próximas tarefas (ordem; cada uma tem um cartão em docs/tasks/)
1. Passo 3 — Runtime Bun + SDK mínimo: `aero.windows.list/focus/close`, snapshot + patches sobre o IPC existente; tipos TS gerados de `aeroshell-ipc`.
2. Passo 4 — Painel React (`useWindows()`), clique foca a janela certa; testar com 2 e 3 janelas.
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
  Conflito: linha 16 manda corrigir o gate se falhar; antiga linha ~94 manda parar. Linhas 3/135 chamam `progress.md` de fonte
  única, mas este arquivo é a fonte única. Há também um título órfão no fim. Limpeza em cartão próprio, NÃO feita aqui.
## 7. Não medido / não validado (não trate como fato)
- Latência de resize com WebKit real; latência de ponteiro; qualquer desempenho em GPU real (tudo foi em VM sem 3D).
- Número de processos/PSS com `related-view` foi medido só em VM; ADR 0001 está "aceito provisoriamente".
- Estabilidade do `ext-background-effect` (blur): não implementado; Smithay 0.7 não tem handler.
## 8. Ambiente
Ubuntu 26.04 em VirtualBox (sem 3D), host pode ser X11. WebKitGTK 6.0 (2.52.x), gtk4-layer-shell, Rust via rustup, Bun.
WPE ausente no Ubuntu 26.04. Alvo de pacote: Debian 13 (trixie) + Ubuntu 26.04.
 
## 9. Handoff (reescreva a cada sessão, 10 linhas no máximo)
- O que eu estava fazendo: acrescentei `AGENTS.md §0.9` (juiz humano para validação visual) + 1 linha no relatório `§0.7` e 1 no checklist `§9`.
- O que funcionou: edição aplicada. `git diff -- AGENTS.md` contra HEAD inclui os 3 acréscimos desta tarefa **e** a reescrita do §0 feita pelo usuário (ainda não commitada). Nada foi descartado; nenhum código tocado.
- Comando exato para reproduzir o estado atual: `git status --short && git log --oneline -3 && git diff -- AGENTS.md docs/STATE.md && bash scripts/check.sh`
- Próximo passo imediato (uma ação): executar o bootstrap já aprovado — criar cartões `00`, `02b`, `03`; preencher o hash `3d52160`, o §4 (window_id quebrado na `wip/window-id`) e o §9; alinhar `RUNBOOK.md:12`; corrigir/commitar `docs/progress.md`.
- Descobertas laterais registradas na seção 6 (não corrigidas): protocolos duplicados no `AGENTS.md` e `progress.md` ainda citado como fonte única.
 