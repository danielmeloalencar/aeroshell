# ADR 0003 — Blur-behind: apenas ext-background-effect-v1

- Status: aceito (F0/S5)
- Data: 2026-11-18 (revisado após correção do usuário)

## Contexto
A versão anterior do `aero_frame_v1` tinha `set_blur_region` privado e dizia "usar o padrão
quando disponível, privado como fallback". Isso é inválido num compositor próprio: **nós**
decidimos o que está disponível; duas rotas significam dois códigos para testar e manter.

## Evidência
```
$ pkg-config --modversion wayland-protocols
1.47
$ find /usr/share/wayland-protocols -iname '*background*'
/usr/share/wayland-protocols/staging/ext-background-effect/ext-background-effect-v1.xml
$ grep -rn "background_effect\|BackgroundEffect" smithay-0.7.0/src   # vazio
```
O Smithay 0.7.0 **não** tem handler pronto para `ext_background_effect_v1`.

## Decisão
- `set_blur_region` foi **removido** de `aero_frame_v1` (XML v1 revisado).
- O único caminho é `ext_background_effect_v1`. O compositor implementará o handler
  (`ext_background_effect_manager_v1` + `ext_background_effect_surface_v1`) no F1/F7.
- O shell (GTK4) obtém a `wl_surface` via `gdk4wayland::WaylandSurfaceExt::wayland_surface()`
  (crate `gdk4wayland`, mesma versão 0.11 do GTK4) e cria o
  `ext_background_effect_surface_v1` sobre ela.
- O protocolo está em **staging**: mudanças futuras são risco registrado, não motivo para
  manter um segundo caminho. Se mudar, atualizamos um único handler.

## Esforço estimado
Gerar o código com `wayland-scanner`, implementar `GlobalDispatch` + `Dispatch` no compositor,
armazenar a região por superfície e aplicá-la no pipeline de render (Smithay já aceita
`opaque_regions` no `RenderElement::draw`; blur real exigirá shader/textura adicional). Fora do
escopo do protótipo mínimo do frame.
