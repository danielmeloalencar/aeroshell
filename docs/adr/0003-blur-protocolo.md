# ADR 0003 — Blur-behind: protocolo padrão vs request privado

- Status: aceito (F0/S5)
- Data: 2026-11-18

## Contexto
O `aero_frame_v1` original propunha `set_blur_region(x,y,w,h)` privado. Antes de manter isso,
verifiquei o que já existe no wayland-protocols instalado.

## Evidência
```
$ pkg-config --modversion wayland-protocols
1.47
$ find /usr/share/wayland-protocols -iname '*background*' -o -iname '*blur*'
/usr/share/wayland-protocols/staging/ext-background-effect/ext-background-effect-v1.xml
$ grep -n '<interface\|<request' .../ext-background-effect-v1.xml
<interface name="ext_background_effect_manager_v1" version="1">
<request name="get_background_effect">
<interface name="ext_background_effect_surface_v1" version="1">
<request name="destroy" type="destructor">
<request name="set_blur_region">
```

Existe, portanto, um protocolo padrão em *staging*: `ext-background-effect-v1`, com
`set_blur_region`. Ele é a escolha correta quando disponível.

## Decisão
- Manter `set_blur_region(region: wl_region)` no `aero_frame_v1` **apenas como fallback**.
- O compositor deve preferir `ext_background_effect_manager_v1` quando o cliente o suportar;
  o request privado existe para versões sem o protocolo padrão.
- O XML documenta isso no description do request.

## Consequência
Quando o suporte ao protocolo padrão for implementado (F1/F7), o shell negocia
`ext-background-effect-v1` e deixa de usar o request privado; nenhuma mudança de API do
`aero_frame_v1` é necessária além de parar de chamar o fallback.
