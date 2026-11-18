# ADR 0003 — Blur-behind: apenas ext-background-effect-v1

- Status: aceito (F0/S5); revisado 2026-11-18 após correção do veredito
- Data original: 2026-11-18

## Contexto
A versão anterior do `aero_frame_v1` tinha `set_blur_region` privado e dizia "usar o padrão
quando disponível, privado como fallback". Isso é inválido num compositor próprio: **nós**
decidimos o que está disponível; duas rotas significam dois códigos para testar e manter.
`set_blur_region` foi removido do XML.

## Evidência corrigida
- `smithay 0.7.0` (crate publicado, 24/jun/2025) **não** tem `smithay::wayland::background_effect`.
- **git master do Smithay já tem** o módulo: `BackgroundEffectState`, `ExtBackgroundEffectHandler`,
  `Capability::Blur`, `BackgroundEffectSurfaceCachedState` (double-buffered), com
  `BackgroundEffectState::new` + `delegate_dispatch2!`
  (https://smithay.github.io/smithay/smithay/wayland/background_effect/index.html).
- `wayland-protocols 0.32.13` (já no `Cargo.lock`) traz os bindings gerados em
  `wayland_protocols::ext::background_effect::v1` (feature `staging`). Não é preciso gerar o
  protocolo à mão.
- `RenderElement::draw(frame, src, dst, damage, opaque_regions)` aceita `opaque_regions`
  no 0.7.0 — mas blur real exige render offscreen + shader multipass, e damage tracking com blur
  é conhecidamente difícil (issue Smithay #1363).
- Prior art: o **niri** (baseado em Smithay) implementou blur + `ext-background-effect` em 2026,
  inclusive a otimização "xray blur" (borrar o wallpaper uma vez e reutilizar).

## Decisão revisada
- Um único caminho: `ext_background_effect_v1`. Nenhum fallback privado.
- O custo do **protocolo** é baixo se usarmos Smithay git (handler pronto) ou mesmo gerando
  com `wayland-protocols`; o custo real está na **integração de render**, não no handshake.
- Para o F0 isso continua **adiado**, mas a justificativa correta é: "o blur depende de pipeline
  de render offscreen/shader e de damage tracking; é esforço **médio**, com prior art no niri,
  não 'implementar protocolo do zero'".
- **Dependência**: considerar trocar `smithay = 0.7.0` por uma revisão fixa do repositório
  oficial (https://github.com/Smithay/smithay) ao final do F0/início do F1, para usar o handler
  pronto. Isso será registrado como ADR separado com a rev exata; não alterar o workspace agora,
  durante a validação do protótipo do frame.

## Risco
`ext-background-effect-v1` está em **staging** e pode mudar. Com uma única implementação
(handler do Smithay git), uma mudança de protocolo é atualização de dependência, não manutenção
de duas rotas paralelas.
