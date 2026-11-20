# Tarefa 02p — canvas fixo do frame (adoção do Plano B)

> **FECHADA em 2026-11-20:** baseline full-res falhando; 3 finais OK; growth OK; vizinhos/gate/clippy
> verdes; juiz visual humano: "teste visual ok. Passou". Evidências em `docs/STATE.md` §3.
>
> Autorização explícita do humano em 2026-11-20: mudar a arquitetura do frame para canvas de
> tamanho FIXO e adicionar `frame_painted(serial)` ao protocolo privado pré-estável.
> A causa-raiz apontada pelo humano é que `WaylandSurfaceRenderElement::from_surface` relê o buffer
> CURRENT em `winit.rs`; no release chega buffer no tamanho novo com layout WebKit antigo e o
> 9-slice fatia esse buffer transitório. A correção elimina a classe de transientes por construção.

## Objetivo (uma frase, verificável)
Durante press/arrasto/release de resize, o frame é SEMPRE composto por 9-slice de um canvas WebView
que não muda de tamanho, e a titlebar não apresenta diferença de pixels entre frames de mesmo target,
provado por teste full-res visto falhando no baseline e pelo juiz visual com 2 `foot`.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-shell/examples/frame_webview.rs:249-283,324-445` — configure atual chama
  `set_default_size`; HTML e ciclo de vida da WebView/canvas.
- `crates/aeroshell-compositor/src/backend/winit.rs:228-237,244-313,421-514` — `from_surface`,
  target, `ReleaseSwap`, 9-slice e caminho nativo a aposentar.
- `crates/aeroshell-compositor/src/state.rs:38-68,169-175,426-466` — `ReleaseSwap` e timers
  200/50/400 ms.
- `crates/aeroshell-compositor/src/grab.rs:403-443` — release atual cria `ReleaseSwap`.
- `crates/aeroshell-compositor/src/protocols/aero_frame.rs` — duplo configure, ack e handlers.
- `protocols/aero_frame_v1.xml` — PROTEGIDO; adição de `frame_painted(serial)` autorizada.
- `crates/aeroshell-compositor/src/frame_pixel_probe.rs` — readback Smithay 0.7 da 02o; a grade
  64x40 foi cega ao flicker e não serve como critério desta tarefa.
- ADRs relevantes: 0001 (atlas chamado Plano B), 0004 (geometria anterior), 0006 (escala elástica).
  A adoção do canvas fixo será registrada em ADR NOVO, sem editar os anteriores.

## Critério de aceite (comandos exatos + saída esperada)
1. **Teste PRIMEIRO visto falhando:** `python3 tests/integration/frame_fixed_canvas_release_pixels.py`
   captura a região da titlebar em FULL res e compara frames consecutivos de MESMO target no release.
   Baseline: `VEREDITO: FALHA` com transiente de pixels cru preservado. Depois: 3 execuções seguidas
   com `changed_pixels=0`, `native_frames=0`, `canvas_changes=0` e `VEREDITO: OK`; nenhuma amostra
   descartada nem tolerância ajustada depois da medição.
2. Canvas inicial >= output + insets. Resize comum NÃO chama `set_default_size`; buffer/canvas fica
   constante e TODOS os frames associados usam 9 fatias com caps nas bordas do target. Crescimento só
   quando `target > canvas`, com `frame_painted(serial)` após double-rAF e serial válido; serial antigo
   ou desconhecido é ignorado. `ack_configure` continua imediato e não representa paint.
   `python3 tests/integration/frame_fixed_canvas_release_pixels.py growth` → crescimento real,
   `growth_completed>=1`, `changed_pixels=0`, `native_frames=0`, `VEREDITO: OK`.
3. `ReleaseSwap`, sua geração e os timers 200/50/400 ms são removidos. O configure final pode ser
   imediato para bookkeeping, mas não redimensiona o canvas. Zero configures da decoração no grab.
4. Vizinhos: `frame_grab_configure_hygiene.py`, `frame_configure_hygiene.py`,
   `frame_resize_glue.py`, `frame_initial_buffer.py`, `frame_buttons.py`, `frame_window_id.py`,
   `frame_follows_resize.py`, `frame_resize_edges.py`, `frame_repeated_activate.py` e
   `frame_diagnostics.py normal|faults` OK; `frame_resize_sync.py` permanece FALHA esperada
   (métrica antiga, fora de escopo). Testes que
   exigiam swap para nativo serão substituídos pelas invariantes do canvas fixo, com justificativa.
5. `bash scripts/check.sh` → `OK: scripts/check.sh passou`;
   `cargo clippy --workspace --all-targets --features bench -- -D warnings` → EXIT=0.
6. **JUIZ VISUAL obrigatório:** nested do RUNBOOK com 2 `foot`, aplicação deixada ABERTA; humano faz
   press sem arrasto, arrasto lento/rápido e release nas bordas das duas janelas, sem flicker, e testa
   move/botões. Só fecha com "ok" explícito; quem encerra a aplicação é o humano (AGENTS.md §0.9).
7. Sem conclusão de desempenho: VM sem 3D. Atualizar `docs/STATE.md` §3/§4/§5/§6/§7/§9.

## Decisão de implementação
- Shell: canvas inicial output + insets; realize/associate imediato, present somente após tamanho
  conhecido (não adiar associate, para não repetir o deadlock da 02m). Título/botões/bordas nas bordas
  do canvas. Repaint in-place; `set_default_size` apenas para crescimento real.
- Compositor: SEMPRE 9-slice para o target, reutilizando 02k/02l sem mudar seus cortes. O duplo
  configure informa conteúdo/target no protocolo aero e canvas no layer-shell. Crescimento com no
  máximo um serial pendente, paint + buffer correspondente; sem timeout que force transição.
- Input: mapear caps direito/inferior do target às bordas do canvas para preservar botões e edges;
  a política de move/resize e o target não mudam.
- `frame_painted` pode ser emitido após todo configure; só o serial de crescimento pendente o consome.

## Fora de escopo (NÃO faça)
- Mudar move, conteúdo (não estica), insets ou o algoritmo de 9-slice da 02k/02l.
- Editar ADRs anteriores, `AGENTS.md`, `rust-toolchain.toml`, `Cargo.lock`, segurança ou packaging.
- Reabrir 02c/02e/02f/02j/02k/02l; ajustar limiar da grade 02o; tirar conclusão de desempenho.
- Dependências novas, WPE, Chromium/CEF, comandos fora do repositório ou encerrar instância do humano.

## Limites
- Tempo: ~2 h sem resultado visível → pare e descreva o bloqueio.
- Regra das duas tentativas ativa; duas falhas da mesma abordagem → pare e peça decisão.
- Nenhuma dependência nova. Arquivos protegidos alterados somente na medida autorizada acima.

## Entrega
Relatório: resultado · teste manual · juiz visual · provas cruas · NÃO executado/medido · descobertas
laterais · próximo passo. Atualizar `STATE.md`; `docs/progress.md` não é atualizado.
