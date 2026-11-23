# Tarefa F1-dmabuf — linux-dmabuf-v1 com tranches e import de buffer (RASCUNHO — aguarda "ok")

> Cartão 4 da sequência F1-final aprovada. Divide o rascunho `F1g-dmabuf-presentation.md` (que cobria dmabuf +
> presentation) em dois cartões; abrir só depois do cartão 3 fechado pelo juiz.

## Objetivo (uma frase, verificável)
O compositor anuncia `linux-dmabuf-v1` com tranches reais de formatos/modifiers, aceita importar um buffer DMA-BUF de
cliente e o compõe sem alterar geometria/9-slice, provado por cliente real, com gate verde e regressão zero.

## Contexto mínimo — premissas CONFIRMADAS na fonte fixada (Smithay 0.7.0)
- `wayland/dmabuf/mod.rs`: `DmabufState::new()` (`:588`), `DmabufFeedbackBuilder::new(dev_t, formats)` (`:315`),
  `add_preference_tranche` (`:341`), `build()` (`:390`), `create_global_with_default_feedback` (`:645`),
  `DmabufHandler: BufferHandler` (`:992`) com `dmabuf_imported` (`:1002`), `delegate_dmabuf!` (`:1037`).
- Formatos vêm de `renderer.dmabuf_formats()` (`backend/renderer/mod.rs:618`); não inventar lista.
- Domicile (guia, não receita): `packages/domicile-compositor/src/dmabuf_import.rs:177-198` — anunciar só formatos
  importáveis e ter fallback explícito quando não há EGL.
- VM sem 3D: validar **funcionamento**, não desempenho.

## Critério de aceite
1. Teste-first: cliente real com `zwp_linux_dmabuf_v1` escrito antes; baseline registry sem o global → `VEREDITO: FALHA`.
2. `linux-dmabuf-v1` no registry (cliente real + sonda bench); tranches com formatos/modifiers do renderer.
3. Cliente dma-buf mapeia toplevel com os pixels corretos, 0 erros de protocolo; 9-slice/escala intactos
   (`fractional_scale.py` em 1.0/1.25/1.5 → OK).
4. Regressões: `headless_render_damage.py`, `frame_resize_glue.py`, `frame_buttons.py`, `frame_window_id.py`,
   `frame_fixed_canvas_hygiene.py`, `multi_output_damage.py` → OK.
5. Gate: `check.sh`, clippy `--features bench -D warnings`, `--no-default-features`; nested 1.5 e DRM 1.0/1.5
   (juiz visual do humano no DRM).
6. `ImportNotifier::successful()` chamado uma vez; não reimportar no handler (o `ImportAll` faz o import no render).

## Fora de escopo
- Presentation-time (cartão 5), `wl_drm` legado, hotplug/multi-GPU, geometria/escala aprovadas, `protocols/*.xml`,
  ADR, dependências novas, sudo/VT/push.

## Limites
- Tempo: 2 h sem cliente dma-buf funcional → PARAR e reportar com dados; sem terceira via por conta própria.
- Dependências: nenhuma.

## Entrega
Relatório curto + STATE §3/§7/§9 + commit local. PARAR antes do cartão 5.
