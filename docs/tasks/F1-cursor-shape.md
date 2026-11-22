# Tarefa F1-cursor-shape — subcartão de `F1-gap-protocolos.md` (RASCUNHO — aguarda "ok")

> Um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md,
> `docs/tasks/F1-gap-protocolos.md` e este arquivo. Uma tarefa por sessão; se o objetivo mudar, PARE e pergunte.

## Objetivo (uma frase, verificável)
O compositor anuncia `wp_cursor_shape_v1`, recebe e armazena a forma pedida pelo `foot`, e desenha
o cursor no backend udev — com o aviso "server-side cursors" ausente e o cursor visível no juiz humano,
sem regredir o conteúdo já aprovado na F1d3.

## Contexto mínimo — premissas CONFIRMADAS na fonte fixada (Smithay 0.7.0)
Antes de qualquer edição, foi lido o código da versão fixada; nada aqui é chute de API:

- `wayland/cursor_shape.rs:124-145` — `CursorShapeManagerState::new::<D>(&dh)` cria o global (versão 2);
  exige `D: GlobalDispatch + Dispatch + SeatHandler`.
- `wayland/cursor_shape.rs:239-279` — o `Dispatch` do device exige **`D: SeatHandler + TabletSeatHandler`**
  e `<D as SeatHandler>::PointerFocus: WaylandFocus`; em `:277-278` converte a forma e chama
  `state.cursor_image(&seat, CursorImageStatus::Named(icon))`.
- `wayland/cursor_shape.rs:362-373` — `delegate_cursor_shape!` gera 3 `delegate_dispatch`.
- `wayland/tablet_manager/tablet_seat.rs:42-44` — `TabletSeatHandler::tablet_tool_image` tem corpo **default**,
  logo `impl TabletSeatHandler for AeroState {}` resolve o bound sem código extra.
- `input/pointer/cursor_image.rs:31-43` — `CursorImageStatus::{Hidden, Named(CursorIcon), Surface(WlSurface)}`;
  `:45-49` traz `default_named()`. `input/pointer/mod.rs:16` reexporta `CursorIcon` (tipo sem dep direta nova).
- `backend/renderer/element/memory.rs:463-489` — `MemoryRenderBufferRenderElement::from_buffer(..., kind: Kind)`
  aceita explicitamente `Kind::Cursor`.
- `backend/drm/compositor/mod.rs:3005-3033` — `try_assign_cursor_plane` só aceita elementos com
  `Kind::Cursor` e que caibam no `cursor_size`.
- **Correção de uma premissa do pedido:** o cursor plane **já está habilitado no nosso código**.
  `backend/udev.rs:146-153` chama `DrmOutputManager::new(..., Some(gbm), ...)`, e o Smithay
  (`backend/drm/output.rs:258-271`) passa `cursor_size` + `gbm.clone()` ao `DrmCompositor::new`.
  Portanto o item (d) **não** é reconfigurar o plane/BO — é só incluir um elemento `Kind::Cursor`
  na cena do udev (`backend/udev.rs:401-413`, via `render::scene::collect_output_elements`).

Código do repo a tocar:
- `crates/aeroshell-compositor/src/state.rs:218-234` — registro dos globals e criação do seat.
- `crates/aeroshell-compositor/src/protocols/seat.rs:21-49` — `SeatHandler::cursor_image` é **no-op** em `:30-35`.
- `crates/aeroshell-compositor/src/render/elements.rs:41-47` — `AeroRenderElements` não tem variante de cursor.
- `crates/aeroshell-compositor/src/backend/udev.rs:401-413` — ponto de injeção do elemento no render.

## Escopo (exatamente quatro partes)
- **(a)** `impl TabletSeatHandler for AeroState {}` (bound de `delegate_cursor_shape!`).
- **(b)** Campo em `AeroState` para o `CursorImageStatus` recebido; `cursor_image` deixa de ser no-op e
  grava o valor. Log sob `cfg(feature = "bench")`: forma/tamanho + origem (`Named`/`Surface`/`Hidden`),
  sem imprimir nada em produção.
- **(c)** `CursorShapeManagerState::new::<AeroState>(&dh)` no `state.rs` + `delegate_cursor_shape!(AeroState)`.
- **(d)** Elemento `Kind::Cursor` na cena do udev (posição do ponteiro + hotspot), usando o cursor plane
  já habilitado. Inclui a variante correspondente em `AeroRenderElements` sem alterar a ordem z nem o 9-slice.

## Critério de aceite (comandos exatos + saída esperada)
1. **Teste-first (pré-filtro executável nesta VM):** com o nested do RUNBOOK e `foot` conectado ao socket
   do AeroShell, capturar `stderr` do `foot` ANTES da mudança → mensagem de cursor presente (rascunho 05,
   mensagem 5: "compositor does not implement server-side cursors, falling back to client-side cursors").
   Depois da parte (c), o mesmo comando → mensagem **ausente**. Output cru colado no relatório.
   **Nota:** o critério literal do pedido é "no tty3"; esta VM não tem VT e eu não posso operar o tty3
   (ver § Juiz visual). O nested mede o MESMO global do protocolo; o veredito no tty3 é do humano.
2. **Instrumentação:** com `--features bench`, o log contém `CURSOR_SHAPE …` com a forma recebida e o
   tamanho do buffer/hotspot; sem `bench`, `strings` do binário NÃO contém esse prefixo.
3. **Cursor visível no udev:** juiz humano no tty3 (§ Juiz visual) responde "ok" — nenhuma sentença
   automatizada substitui isso.
4. **Não regressão:** `bash scripts/check.sh` verde; `cargo clippy --workspace --all-targets -- -D warnings`
   EXIT=0 (e com `--features bench`); `cargo check --workspace --no-default-features` EXIT=0; os harnesses
   de frame do RUNBOOK (`frame_buttons`, `frame_window_id`, `frame_fixed_canvas_hygiene`,
   `headless_render_damage`) seguem OK — **nada mexe no bug do conteúdo F1d3**.
5. **Commits:** um commit para (a)+(b)+(c) com gate verde, outro para (d) com gate verde (regra 10).
   Sem push.

## Juiz visual e limites do agente (CONFLITO a confirmar)
O pedido diz "você testa no tty3 (`~/u`, move o mouse, me diz se o cursor aparece)" **e** também
"Sem sudo/VT". São instruções incompatíveis na prática:
- nesta VM o `--backend udev` falha em `session.open()` com `EAGAIN` (sem VT) — RUNBOOK:64-78;
- AGENTS.md §0.9 e RUNBOOK:93-94 reservam a operação do tty3 e o veredito ao **humano**; o agente não
  troca VT, não usa `sudo`, não toca em `/etc`;
- o próprio RUNBOOK manda o humano operar e fechar a aplicação.

**Padrão proposto:** o agente implementa, instrumenta, roda os gates e deixa o build pronto com o
comando exato (`~/u` no tty3); o humano roda, move o mouse, diz se o cursor aparece e dá o "ok".
O agente **não encerra** a aplicação e **não escreve "concluído"** sem esse ok. Se o humano confirmar
outro arranjo, seguir a decisão dele.

## Decisão pendente (dependência nova — NÃO implementar sem resposta)
Para um cursor **tematizado** no estilo anvil é preciso carregar uma imagem de tema Xcursor
(`xcursor 0.3.11`, MIT, **já no `Cargo.lock` como transitiva**, mas exigiria declaração direta —
AGENTS.md §3.10 pede nome/versão/licença/motivo/alternativa e aprovação).
Alternativa sem dep nova: bitmap RGBA mínimo embutido (seta) + hotspot, suficiente para provar o
encaminhamento `Named → Kind::Cursor → cursor plane`.
**Recomendação:** começar pelo bitmap embutido neste subcartão; trocar por tema Xcursor em cartão
posterior se o juiz quiser. Confirmar antes de escrever código.

## Fora de escopo (NÃO fazer nesta tarefa)
- O bug do conteúdo do `foot`/painel no udev — F1d3 FECHADA; não reabrir, não "aproveitar" para mexer.
- Os outros 6 avisos do `foot` (primary-selection, activation, urgency, fractional-scale, icon, IME).
- Protocolo XML, ADR, lock/PAM/greeter, services/apps, desempenho em VM sem 3D.
- Mudança em `is_empty`, input, VT, vblank, geometria do frame ou 9-slice.

## Se encontrar OUTRO problema
Anotar 1 a 3 linhas em `docs/STATE.md §6` (sintoma, onde, hipótese) e VOLTAR. Só investigar desvio se
bloquear o critério de aceite, e por no máximo 15 minutos. Regra das duas tentativas (AGENTS.md §0.4).

## Limites
- Tempo: se em 2 h não houver cursor visível no juiz, PARAR e reportar com dados — não tentar terceira via.
- Dependências: nenhuma sem a decisão acima; nenhuma mudança fora do repositório.

## Entrega
Relatório curto: o que funciona · como testar à mão (comandos exatos) · o que NÃO foi testado ·
gate colado · descobertas laterais já anotadas. Ao fechar, atualizar `STATE.md §3/§7/§9` e commitar localmente.
