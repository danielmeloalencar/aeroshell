# ADR 0004 — Frame via `associate` sem atribuição de role

- Status: **aceito pelo usuário em 2026-11-18** (emendas incluídas)
- Data: 2026-11-18
- Relação: **substitui o `get_frame` do ADR 0001** sem editar ADRs aceitos. ADR 0001 continua
  valendo para `related-view`, pool e gatilhos do atlas; ADR 0003 continua valendo para blur.

## Contexto

O `aero_frame_v1` original dava `give_role(aero_frame)` à `wl_surface` em `get_frame`.
Isso é incompatível com uma janela GTK4: o GDK4/Smithay já atribuem `xdg_toplevel` ou
`zwlr_layer_surface_v1` ao mapear, e a Wayland permite **um único role por superfície**.
Evidências do spike (documentação/código da versão fixada):

- `/usr/share/wayland/wayland.xml:1418` — "A surface can have only one role at a time... Once a
  wl_surface is given a role, it is set permanently".
- `smithay-0.7.0/src/wayland/compositor/tree.rs:170-177` — `set_role` retorna `AlreadyHasRole`
  para role diferente; `give_role` é esse `set_role` (`compositor/mod.rs:394`).
- `smithay-0.7.0/src/wayland/shell/wlr_layer/handlers.rs:86` — o handler layer-shell já dá
  `LAYER_SURFACE_ROLE`. Logo o `give_role(aero_frame)` antigo rejeitaria o frame do próprio shell.
- `/usr/share/gir-1.0/GdkWayland-4.0.gir:726` (`get_wl_surface`), `:447` (`get_wl_display`) e
  `/usr/share/gir-1.0/Gdk-4.0.gir:32419/32448` (`new_popup`/`new_toplevel`) — o GDK4 **não**
  expõe criação de `wl_surface` sem role.
- `gtk4-layer-shell-0.8.0/src/manual.rs:13-22` — devolve o `zwlr_layer_surface_v1` já existente.

## Opções consideradas

| Opção | Resultado do spike | Desfecho |
|---|---|---|
| **A** — superfície sem role via `gdk4-wayland` | Bloqueio é no GDK: só há construtores com role. Exigiria FFI especulativa e talvez sair do GDK. | **Rejeitada** (não recomendada; nenhuma dependência nova adicionada). |
| **B** — frame como `layer_surface` + `associate` que apenas **marca** | Reaproveita o S2 (layer transparente + WebView) validado; resolve o conflito de role. | **Aprovada com emendas.** |
| **C** — `xdg_toplevel` marcado por `app_id` | `app_id` é do `GApplication`, não por janela; nenhuma API no GDK4 instalado. | **Deferida**: superfícies associadas que viram `xdg_toplevel` recebem `closed`. |

## Decisão (com emendas do usuário)

1. **`get_frame` → `associate(id, surface, window_id)`.** Não chama `give_role`; apenas marca.
   Role-agnostic: `layer_surface` suportado agora; `xdg_toplevel` em superfície associada →
   `closed` + suporte deferido (não entra na lista de janelas).
2. **Regras:** `associate` só ANTES de a superfície ter role (depois → erro `role`); segunda
   associação da mesma superfície **ou** janela → `already_constructed`; window_id inexistente →
   objeto criado + `closed` (corrida legítima mantida); window_id nunca reutilizado.
3. **Exceção ao LayerMap:** a superfície associada NÃO entra no `LayerMap` nem na lista de janelas.
   Geometria 100% do compositor: posição = janela − insets; tamanho = conteúdo + insets +
   shadow_extents. Anchors/exclusive_zone/margins ignorados; keyboard-interactivity `none`;
   z-order junto da janela, abaixo de panels/overlay.
4. **Duplo configure:** o compositor envia `zwlr_layer_surface_v1.configure` (tamanho TOTAL) e
   `aero_frame_v1.configure` (conteúdo + state) na **mesma transação**. `ack_configure` do aero
   continua a gatear a geometria, com timeout de 100 ms.
5. Suporte a `xdg_toplevel` como frame fica **deferido**; GPU real e medições com WebKit ficam
   para depois do spike de código mínimo (passo B).

## Consequências

- XML `protocols/aero_frame_v1.xml` emendado (aprovação explícita do usuário nesta data);
  código servidor/cliente regenerado em tempo de compilação.
- Os testes de protocolo foram reescritos: associate antes do role = caminho feliz; depois = erro.
  Novos casos: dupla associação, associate após role, unknown window → closed, global invisível
  a cliente comum.
- Insets/shadow_extents ainda são apenas registrados (`tracing`) neste passo: o tamanho TOTAL
  do duplo configure é igual ao do conteúdo até a implementação da geometria completa.
