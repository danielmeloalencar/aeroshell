# ADR 0006 — Composição elástica do frame no compositor

- Status: **superado por 0007 em 2026-11-20** (proposto pelo agente em 2026-11-20; aditivo; não edita nem substitui ADR aceito)
- Data: 2026-11-20
- Relação: complementa o ADR 0001 (Plano B leve) e o ADR 0004 (frame = `layer_surface` abaixo do
  cliente). Não altera `aero_frame_v1.xml`, protocolos, fases nem dependências.

## Contexto

Com o ADR 0004 o frame é uma WebViewGTK desenhada pelo próprio cliente shell. No resize, o
tamanho alvo da decoração é `conteúdo_commitado + insets + shadow` (`state.rs:36-56`,
`FrameGeometry::{extra_w, extra_h}`). O cliente recebe o configure, mas o **buffer** da decoração
só aparece depois de um pipeline assíncrono:

1. GTK4 faz size-allocate e envia a nova geometria por IPC ao WebKit;
2. WebKit recomputa o layout e pinta via Skia no `ThreadedCompositor`;
3. o resultado vira DMA-BUF/fence, é importado de volta pelo GTK e redesenha a janela;
4. só então há `wl_surface.commit` na layer surface do frame, no próximo frame-clock.

Fonte: https://docs.webkit.org/Ports/WebKitGTK%20and%20WPE%20WebKit/Graphics.html (consultada
nesta data). O cliente de conteúdo (`foot`) é ~1 frame mais rápido que esse caminho. Consequência
medida: por alguns frames o compositor desenha o buffer **antigo** da decoração com a geometria
**nova** do conteúdo, e a borda "corre atrás" do texto.

Medição crua, sem a correção (`/tmp/opencode/glue_instrumented_sem_escala.log`):

```
FALHA: rescaled=False mas buffer=(708, 530) target=(714, 530); ...
linhas RESIZE_GLUE=286
durante_grab=124 apos_soltar=80 colados=80
VEREDITO: FALHA
```

A métrica relevante é, portanto, **espacial** e não temporal: para cada frame renderizado,
`tamanho_renderizado_do_frame == tamanho_commitado_do_conteúdo + extras (±1 px)`. Exigir que o
buffer do WebKit chegue antes do próximo motion (critério antigo do `frame_resize_sync.py`) é
inalcançável por projeto, porque o pipeline acima tem mais de um frame de latência.

Fonte das APIs (Smithay 0.7.0 fixado, sem inventar API):

- `smithay-0.7.0/src/backend/renderer/element/utils/elements.rs:12-30` — `RescaleRenderElement`
  e `from_element(element, origin, scale)`.
- `smithay-0.7.0/src/backend/renderer/element/surface.rs:269` — `buffer_size()`.
- `smithay-0.7.0/src/backend/renderer/element/mod.rs:492` — macro `render_elements!`.
- Padrão de composição copiado de `anvil v0.7.0 anvil/src/render.rs`
  (https://github.com/Smithay/smithay/blob/v0.7.0/anvil/src/render.rs).

## Decisão

No backend `winit` (`crates/aeroshell-compositor/src/backend/winit.rs`), o frame é importado como
um dos dois variantes de `AeroRenderElements`:

- `Surface` quando `buffer == target`;
- `Rescaled(RescaleRenderElement::from_element(el, loc, scale))` quando divergem, com
  `scale = (target.w / buffer.w, target.h / buffer.h)`.

O alvo usa o conteúdo **já commitado**, não o configure pendente. A escala é aplicada sempre que
há mismatch (não só durante grab), mas o move puro não muda tamanho e portanto não escala. Damage
e o caminho normal permanecem inalterados: `winit.rs` já usa `render_output` com `age=0` e
`request_redraw` incondicional.

O repaint assíncrono do WebKit continua necessário e corre em paralelo; quando ele alcança o
alvo, o próximo render volta automaticamente para `Surface` sem nenhuma transição de estado.

## Alternativas preteridas

| Opção | Por que não foi escolhida |
|---|---|
| Acelerar o repaint do WebKit (terceira variação de ajustes de paint) | É a família que já falhou duas vezes nesta sessão; o gargalo é o pipeline de graphics, não uma config. |
| `wp_viewporter` no shell para escalar a layer surface | Exige negociação com o compositor e muda a geometria do cliente; a escala no render é local, não protocolo, e não mexe em `protocols/*.xml` (congelado na Fase 0). |
| 9-slice com `CropRenderElement` (bordas/cantos fixos, centro esticado) | Preserva texto nítido, mas exige declarar regiões e mais elementos por frame; é o plano de contingência se a distorção de texto for visível (ver gatilhos). |
| Transação atômica: só atualizar a geometria quando o buffer novo chegar | Elimina o descolamento, mas atrasa o conteúdo junto com a decoração; troca um erro visual por outro e piora a responsividade. |
| WPE WebKit | Ausente no Ubuntu 26.04; decisão congelada no ADR 0001 / AGENTS.md §2. |

## Evidência (VM sem 3D — apenas funcionamento/geometria, não desempenho)

- Teste escrito antes da correção: `tests/integration/frame_resize_glue.py`; baseline sem
  instrumentação falhou (`/tmp/opencode/glue_baseline.log`) e, instrumentado sem escala, falhou
  com descolamento real (log acima).
- Após a correção, 3 execuções seguidas, nenhuma amostra descartada:
  `/tmp/opencode/glue_final1.log`, `glue_final2.log`, `glue_final3.log` → todas `VEREDITO: OK`,
  `durante_grab` 182/141/165, `apos_soltar` 89/71/67, `colados` igual a `apos_soltar`.
- Mecanismo de escala exercitado de fato: `/tmp/glue-comp-58730.log`, `-58852.log`, `-58966.log`
  mostram ~104–118 linhas `rescaled=true` por execução.
- Amostra de infraestrutura registrada, não descartada: `/tmp/glue-comp-58443.log` teve 28
  `FRAME_BUFFER_COMMIT`, **todos** com `buffer=None`; o harness agora espera o primeiro
  `FRAME_BUFFER_COMMIT buffer=Some(...)` antes de arrastar.
- Linhas corrompidas por interleaving de stdout (`buffer=818798xx530530`,
  `rescaled=xtrue530`) são contabilizadas como `malformadas=N`, nunca usadas como amostra válida.

## Consequências

- Nenhuma dependência nova, nenhum arquivo protegido alterado, nenhuma mudança de protocolo.
- A escala é uma aproximação visual temporária: texto da barra de título pode ficar levemente
  esticado por poucos frames. Isso é uma troca consciente: borda colada vale mais que nitidez
  transitória do texto durante o arrasto.
- A métrica antiga `frame_resize_sync.py` permanece FALHA por desenho e não é editada; sua
  limitação está registrada em `docs/STATE.md`.

## Gatilhos de reavaliação

1. Se a distorção do texto durante o resize for percebida pelo humano na validação visual,
   implementar 9-slice com `CropRenderElement` e substituir esta decisão por um novo ADR.
2. Se em GPU real (fora desta VM sem 3D) o repaint do WebKit chegar antes do próximo frame,
   medir novamente: a escala pode se tornar desnecessária e ser removida.
3. Se o WebKitGTK fixado mudar de versão, reconsultar o pipeline de graphics antes de manter a
   suposição de latência de múltiplos frames.
