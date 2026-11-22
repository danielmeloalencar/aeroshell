# Tarefa 02q (REABERTA em 2026-11-22) — cold-start da decoração, sem adiamento

> Histórico preservado: cartão fechado em 2026-11-21 por decisão do juiz ("visualmente perfeito e bem
> otimizado"), com o cronômetro >300 ms registrado como lacuna (`STATE.md` §7). O humano reabriu o
> cartão em 2026-11-22 porque vê **1–2 s** entre abrir a janela e a decoração aparecer. Nada do
> fechamento anterior é apagado; esta é uma nova execução com critério de medição diferente.
> Decisão do humano (2026-11-22): **sem gambiarra** — nada de "esperar o associate para nascer o
> cliente". O cliente nasce no startup; a garantia da decoração é do compositor.

## Objetivo (uma frase, verificável)
Toda janela mapeada tem decoração sólida no primeiro render, com o HTML real em ≤300 ms no caminho
quente, sem esperar o WebView associar para o cliente nascer — e o primeiro HTML do caminho frio é
reportado como número separado, sem ser fingido dentro do limite.

## Contexto mínimo
- `crates/aeroshell-compositor/src/main.rs:98-123` — shell e cliente são lançados hoje em série
  (`pending_initial_command` em `state.rs:173`, consumido em `protocols/aero_frame.rs:291-303`).
- `crates/aeroshell-compositor/src/backend/winit.rs:474-481` e `render/scene.rs:277-296` —
  `push_frame_elements` só é chamado se `associated_frames` já tem o cliente; sem associate, a janela
  não tem decoração alguma. `render/elements.rs:411-465` é o placeholder sólido atual.
- `protocols/aero_frame_v1.xml` — APROVADO pelo humano: novo request de manager
  `set_default_insets(l,t,r,b)` no startup, para o compositor ter geometria antes do `associate`.
  É proibido duplicar as constantes do tema HTML (`TITLEBAR_H/BORDER` do shell) no compositor.
- `crates/aeroshell-shell/examples/frame_webview.rs:443-461,523-534,716-733` — âncora,
  pool e `bootstrap_runtime` (hoje com 30× `sleep(2 ms)`: **remover o sleep**, sem espera cega).
- Guia (não receita): `/home/daniel/Projetos/domicile-engine/docs/contributing/COMPOSITOR-DEBUGGING.md:34-48`
  (spawn → connect → drew), `docs/WRITING-A-SHELL.md:410-416` ("listed means drawn, not shown"),
  `docs/architecture/WINDOW-COMPOSITING.md:145-155` (latch ≠ presented).

## Estado da execução (2026-11-22 — PARCIAL, não fechada)
- Baseline 3×: `VEREDITO: FALHA`, `shell_spawn→client_spawn` 1113–1249 ms, latch quente 417–501 ms
  (`/tmp/opencode/02q-reopen/baseline{1,2,3}.log`).
- Depois 3×: `VEREDITO: FALHA` **somente** pelo latch quente 434–537 ms; spawn 2,0–2,5 ms;
  placeholder 7–27 ms; frio 1227–1412 ms reportado à parte; latch→presented 3,7–12,0 ms
  (`/tmp/opencode/02q-reopen/final-{1,2,3}.log`). Gate e vizinhos verdes; juiz visual pendente.
- **Decisão híbrida do humano executada:** configure inicial do xdg-shell com tamanho sugerido 800×600
  (não 0) + `suggested_window_size` no render eliminaram o spike 12×36 — teste-primeiro em
  `spike-baseline.log` (12×36) e depois `spike-after-full.log`/`visual-final3.log` (retângulo real,
  0 ocorrências). ADR 0007 **NÃO** reaberto; limite de 300 ms **NÃO** alterado.
- Finais 3× (`stages-final-{1,2,3}.log`): `VEREDITO: FALHA` somente no latch quente 448–572 ms; frio
  1,2–1,4 s reportado à parte; spawn 2–6 ms; placeholder <100 ms; latch→presented 4–18 ms.
  Vizinhos 6× OK; gate `gate-hybrid.log` EXIT=0; clippy bench 0; `--no-default-features` 0.
- Juiz visual: humano respondeu **"está ok"** na visual4 (barra imediata, sem spike/flash). Falta testar
  max/✕ na `wayland-aero-02q-visual5` (PID 197095, IPC `/tmp/opencode/02q-reopen/visual-ipc-5.sock`) —
  na visual4 os cliques chegaram ao JS mas foram ignorados por falta de `AERO_WINDOW_IPC_SOCK` no comando
  de demonstração (RUNBOOK corrigido). Maximizar não reposicionar a janela é dívida pré-existente (§6),
  fora desta tarefa.
  Pergunta extra ao juiz: o `foot` abrindo em ~800×600 (por causa do configure sugerido) é aceitável?

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste primeiro:** `python3 tests/integration/frame_cold_start_stages.py` no código atual,
   3 execuções seguidas (`/tmp/opencode/02q-reopen/baseline{1,2,3}.log`), nenhuma amostra
   descartada. Baseline deve dar `VEREDITO: FALHA` mostrando: cliente inicial nascendo ~1,2 s
   depois do shell, janela 0 fora da medição antiga e `map → 1º real` >300 ms nas janelas quentes.
2. Depois da correção, o MESMO comando 3× seguidas → `VEREDITO: OK` em todas, com:
   - marcos M0..M12 com `t=` sob `bench`: init, shell_spawn, webprocess, pool_ready, associate,
     client_spawn, client_connect, map, placeholder_pixel, html_finished, latch, presented;
   - `client_spawn − shell_spawn < 100 ms` (sem espera de associate);
   - `map → placeholder ≤ 100 ms` para as 3 janelas, incluindo a janela 0;
   - `map → 1º HTML real ≤ 300 ms` para as janelas 1 e 2 (caminho quente);
   - janela 0 (frio): primeiro HTML **reportado separadamente**, sem limite inventado;
   - `latch` (`RESIZE_GLUE`) e `presented` (`OUTPUT_FRAME`/`UDEV_FRAME`) como números distintos;
   - 1 WebKitWebProcess, âncora viva, pool reposto com 2 views.
3. Vizinhos: `frame_cold_start.py` (harness antigo, preservado), `frame_initial_buffer.py`,
   `frame_fixed_canvas_hygiene.py`, `frame_repeated_activate.py`, `frame_buttons.py`,
   `frame_window_id.py`, `frame_resize_glue.py` → OK.
4. `bash scripts/check.sh` → `OK: scripts/check.sh passou` numa única execução;
   `cargo clippy --workspace --all-targets --features bench -- -D warnings` → 0;
   `cargo check --workspace --no-default-features` → 0.
5. Juiz visual obrigatório: nested pelo RUNBOOK e tty3 via `~/u`, aplicação deixada aberta,
   2–3 `foot`, humano confirma decoração imediata sem gap. Sem o "ok", fica em `STATE.md` §7.

## Fora de escopo
- resize, 9-slice, canvas fixo, grabs, escala, cursor, painel React, protocolos de segurança.
- ADR aceito, `Cargo.lock`, dependências novas, `sudo`/`/etc`, medição de GPU real (VM sem 3D).
- Mudar o harness antigo ou descartar/amaciar amostras; "corrigir" limite para passar.

## Limites
- Uma correção por vez; se a mesma abordagem falhar 2×, parar e pedir decisão.
- Tempo: 2 h sem resultado visível → parar e descrever o bloqueio.
- Se aparecer necessidade de ADR/arquitetura além do `set_default_insets` aprovado: **parar e perguntar**.

## Entrega
Resultado · teste manual exato · confirmação visual · provas cruas antes/depois · não executado/não
medido · descobertas laterais · próximo passo sem executar. Atualizar `STATE.md` §§3–7 e §9;
commit local sem push.
