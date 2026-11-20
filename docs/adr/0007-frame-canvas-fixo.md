# ADR 0007 — Frame WebView em canvas fixo com 9-slice permanente

- Status: **aceito explicitamente pelo usuário em 2026-11-20 (tarefa 02p)**.
- Data: 2026-11-20.
- Relação: implementa o Plano B de buffer sobredimensionado/recorte; não é o atlas do ADR 0001.
  Substitui a geometria "surface = target" do ADR 0004 e a política de composição elástica do ADR
  0006 (proposto), sem editar esses documentos. Mantém `related-view`, layer_surface, insets e o
  9-slice com zonas protegidas das tarefas 02k/02l.

## Contexto

No resize, GTK/WebKit pode publicar um buffer com o tamanho novo e o layout antigo. Como
`WaylandSurfaceRenderElement::from_surface` lê o buffer CURRENT, o compositor fatiava esse buffer
transitório. A 02n tentou temporizar a troca entre 9-slice e nativo (`ReleaseSwap`, 200/50/400 ms),
mas o juiz visual continuou vendo flicker e a grade 64x40 da 02o não o capturou.

Uma sonda full-res da titlebar, executada antes desta mudança, confirmou o transiente rápido:
`/tmp/opencode/02p_pixel_baseline3.log` registrou dois pares com target constante e **1.910 pixels
RGB alterados em cada** (`changed_pixels=3820`), incluindo a transição do buffer 708×530 para
768×582. O usuário também relatou ter visto o flicker durante a execução instrumentada.

## Decisão

1. Cada frame WebView tem um **canvas fixo**, inicialmente >= output + insets + shadow. O target de
   apresentação continua `conteúdo_commitado + insets + shadow`; conteúdo, move e insets não mudam.
2. Resize comum **não chama `set_default_size`** e não redimensiona a WebView. Título, tema e outros
   repaints ocorrem in-place. Título fica na borda esquerda do canvas; botões e caps direito/inferior
   ficam nas bordas correspondentes do canvas.
3. O compositor usa **sempre 9-slice** do canvas para o target, inclusive quando os tamanhos são
   iguais. Não há transição para render nativo. Reutiliza os cortes e deslocamentos da 02k/02l.
4. `aero_frame_v1.configure` informa o conteúdo; `zwlr_layer_surface_v1.configure` informa o canvas.
   `ack_configure` continua imediato e significa somente recebimento. O novo request
   `frame_painted(serial)` é enviado após paint real (double-rAF → postMessage).
5. Se `target > canvas`, há no máximo um crescimento pendente. Só o `frame_painted` com o serial
   daquele crescimento o conclui; serial antigo, desconhecido ou duplicado não altera estado. Não há
   timeout que force uma transição visual.
6. O hit-test continua limitado ao target. Para input nos caps direito/inferior, o compositor mapeia
   as coordenadas para as bordas do canvas, preservando botões HTML e as 8 bordas/cantos de resize.
7. `ReleaseSwap`, sua geração e os timers 200/50/400 ms são removidos. O configure final no release
   é apenas bookkeeping e não redimensiona o canvas.

## Evidência (VM sem 3D; funcionamento/pixels, não desempenho)

- Baseline full-res falhando: `/tmp/opencode/02p_pixel_baseline3.log` → `VEREDITO: FALHA`,
  `changed_pixels=3820`, `native_frames=107`, `canvas_changes=1`.
- Depois, três execuções completas do mesmo harness:
  `/tmp/opencode/02p_pixel_after2.log`, `02p_pixel_after3.log` e `02p_pixel_after4.log` → todas
  `VEREDITO: OK`, 8/8 motions, target final 768×582, `changed_pixels=0`, `native_frames=0` e
  `canvas_changes=0`. Nenhuma amostra foi descartada.
- Crescimento real: `/tmp/opencode/02p_pixel_growth1.log` → canvas 1292×836 → 1344×1167,
  `growth_completed=1`, `changed_pixels=0`, `native_frames=0`, `VEREDITO: OK`.
- Protocolo: `frame_painted_only_completes_matching_growth_serial` prova que serial antigo não
  conclui growth, o serial correto conclui uma vez e duplicado não recria estado.
- Botões e oito bordas/cantos passam com o mapeamento target→canvas. O teste 02n foi substituído por
  `frame_fixed_canvas_hygiene.py`: `frames_fora_9slice=0`, `canvas_changes=0`,
  `release_swap_events=0`.

## Consequências

- O WebKit sai do caminho crítico do resize comum: commits de repaint in-place são válidos porque o
  canvas não muda. Crescimento excepcional continua serializado por paint.
- O buffer inicial é dimensionado antes do `present`; o `associate` não é adiado, evitando o deadlock
  da 02m. `get_registry` tem flush explícito para não depender do antigo `present` para enviá-lo.
- `FRAME_STRETCH_ACTIVE` passa a delimitar o episódio de escala ao grab interativo, pois 9-slice
  permanente não tem mais transição nativa que o encerre.
- Desempenho, PSS e latência em GPU real permanecem **não medidos**. O juiz visual testou a instância
  v2 com 2 `foot` e respondeu **"teste visual ok. Passou"** em 2026-11-20; o próprio usuário a encerrou.
- A reconciliação dos insets iniciais preserva o canvas igual ao do shell. A inicialização do
  `GApplication` é idempotente: segunda ativação é ignorada e não reassocia/derruba os frames.
