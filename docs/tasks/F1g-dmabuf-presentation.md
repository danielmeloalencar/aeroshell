# Tarefa F1g — dmabuf + presentation-time (RASCUNHO — aguarda "ok")

> Subcartão de `docs/tasks/F1-gap-protocolos.md`. Aberto depois de F1f FECHADA pelo juiz em 2026-11-22
> (`~/u --scale 1.5` e `2` aprovados; commit `242b918`). Um cartão por sessão; PARE antes de F1h/subsurfaces.

## Objetivo (uma frase, verificável)
O compositor anuncia `linux-dmabuf-v1` (com tranches de formatos/modifiers) e `wp_presentation`, aceita
importar buffers DMA-BUF do cliente (ex.: frame do WebKit) sem mudar geometria/9-slice, e entrega
`presentation_feedback` por output — com prova por cliente real e regressão zero em nested/headless/DRM.

## Contexto mínimo — premissas CONFIRMADAS na fonte fixada (Smithay 0.7.0)
**dmabuf** (`wayland/dmabuf/mod.rs`):
- `DmabufState::new()` (`:588`), `DmabufFeedbackBuilder::new(main_device: dev_t, formats)` (`:315`),
  `add_preference_tranche` (`:341`), `build()` (`:390`), `DmabufState::create_global_with_default_feedback`
  (`:645`). Documentação: `:14-16` diz que a lista de formatos vem de
  `ImportDma::dmabuf_formats` (`backend/renderer/mod.rs:618`, com `has_dmabuf_format` em `:623`).
- `DmabufHandler: BufferHandler` (`:992`) com `dmabuf_state()` (`:994`), `dmabuf_imported(global, dmabuf,
  notifier)` (`:1002`) e `new_surface_feedback` com default (`:1011`). Exemplo canônico em `:51-77`:
  `notifier.successful::<State>()` (`:881`); `ImportAll::import_buffer` faz a importação no render, então
  não reescrever o `WaylandSurfaceRenderElement`.
- `delegate_dmabuf!` (`:1037`). `SurfaceDmabufFeedbackState::from_states`/`set_feedback` (`:505,513`).

**presentation** (`wayland/presentation/mod.rs`):
- `PresentationState::new::<D>(&dh, clk_id)` (`:97`) — global versão 2; `delegate_presentation!` (`:351`).
- Uso (`:28-69`): drenar `PresentationFeedbackCachedState.callbacks` por superfície com `with_states`
  e, após a apresentação, `feedback.presented(&output, time, Refresh::fixed(refresh), seq,
  wp_presentation_feedback::Kind::Vsync)` (`:59-69`). `Refresh` e `PresentationFeedbackCachedState`
  são públicos (`:295`).

**Repo:** `state.rs` registra globals; `protocols/compositor.rs::commit` já chama
`on_commit_buffer_handler`. O frame é desenhado por `render::elements::push_frame_elements` — **não mexer**.

## Escopo (mínimo)
1. Campos em `AeroState`: `DmabufState`, `PresentationState`, `DmabufGlobal` (quando criado).
   `impl BufferHandler` (se ausente), `impl DmabufHandler` (`successful()` sem importar 2× — o import
   acontece via `ImportAll` no render), `delegate_dmabuf!`, `delegate_presentation!`.
2. Criar o global dmabuf **depois** do renderer existir, em cada backend, com formatos reais de
   `renderer.dmabuf_formats()` e `main_device` vindo de `/dev/dri/renderD128` (fallback explícito `0`).
   Sem mudar geometria, insets, slices, viewport ou escala.
3. Presentation: drenar callbacks das superfícies (janelas + frames + layers) e chamar `presented`
   no ponto de apresentação de cada backend — headless após `render_output`/submit, winit após `submit`,
   udev no vblank (`frame_submitted`). Log sob `bench`: `PRESENT_FEEDBACK t=… output=… seq=… n=…`.
4. Teste-first: cliente real com buffer dma-buf (ex.: `wayland-client` mínimo criando `zwp_linux_dmabuf`
   ou o próprio WebKit/GL cliente) e cliente `wp_presentation`, escritos antes e vistos falhando.

## Critério de aceite
1. `linux-dmabuf-v1` e `wp_presentation` aparecem no registry (prova com cliente real + log bench).
2. Cliente dma-buf mapeia toplevel com o conteúdo correto (pixels), sem protocol error; 9-slice intacto.
3. `presented` entregue com `seq` monotônico por output (número de callbacks > 0 no log).
4. Regressão: `fractional_scale.py` em 1.0/1.25/1.5, `headless_render_damage`, `frame_resize_glue`,
   `frame_buttons`, `frame_window_id`, `frame_fixed_canvas_hygiene`, `multi_output_damage` → OK;
   nested 1.5 e DRM 1.0/1.5 (juiz visual do humano para o DRM).
5. Gate: `bash scripts/check.sh` verde; clippy `--features bench -D warnings` EXIT=0;
   `cargo check --workspace --no-default-features` EXIT=0; uma execução de gate pós-código.

## Juiz visual
DRM e aparência do frame com dma-buf: humano decide (AGENTS.md §0.9); agente não abre/fecha app nem troca VT.

## Como usar o domicile — guia, NÃO receita
Invariante transferível: quem detém o ack/present (`DrmCompositor`/vblank) é quem deve responder
`wp_presentation_feedback` e `wl_surface.frame`; diagnósticos nomeados para "por que nada foi apresentado".
**Não copiar** Chromium/Ozone/fork. Fonte executável: Smithay 0.7.0 fixado, citado acima.

## Fora de escopo
- subsurfaces (resto da F1e, fica pendente para depois da F1g); `wl_drm` legado; lock/PAM/greeter;
  geometria/9-slice/escala aprovados; `protocols/*.xml`, ADR, dependência nova, sudo/VT/push, F1h.

## Se encontrar OUTRO problema
Anotar 1–3 linhas em `docs/STATE.md §6` (sintoma, onde, hipótese) e VOLTAR; máx. 15 min de desvio.

## Limites
- Tempo: 2 h sem cliente dma-buf funcional → PARAR e reportar com dados; sem terceira via por conta própria.
- Dependências: nenhuma.

## Entrega
Relatório curto + `STATE.md §3/§7/§9` + commit local. PARAR antes de subsurfaces/F1h.
