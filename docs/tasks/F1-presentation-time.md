# Tarefa F1-presentation-time — wp_presentation com feedback por output (RASCUNHO — aguarda "ok")

> Cartão 5 da sequência F1-final aprovada. Divide o rascunho `F1g-dmabuf-presentation.md`; abrir só depois do cartão 4.

## Objetivo (uma frase, verificável)
O compositor anuncia `wp_presentation` e entrega `presentation_feedback` por output com `seq` monotônico no ponto de
apresentação de cada backend (headless/winit/udev), provado por cliente real e sem regressão de frame.

## Contexto mínimo — premissas CONFIRMADAS na fonte fixada (Smithay 0.7.0)
- `wayland/presentation/mod.rs`: `PresentationState::new::<D>(&dh, clk_id)` (`:97`), `Refresh` (`:203`),
  `feedback.presented(&output, time, Refresh::fixed(refresh), seq, Kind::Vsync)` (`:233`), `delegate_presentation!` (`:351`).
- Drenar `PresentationFeedbackCachedState.callbacks` por superfície com `with_states` antes de apresentar.
- `backend/udev.rs` hoje usa `queue_frame(())` sem `OutputPresentationFeedback` (dívida em `STATE.md §6`); o ponto de
  apresentação é o vblank/`frame_submitted`.
- Domicile (guia, não receita): `packages/domicile-compositor/src/frame_report.rs:52` — quem detém o `present` é quem
  responde `wp_presentation`; `queued=ok`/`submitted=ok` medem enqueue, não apresentação.

## Critério de aceite
1. Teste-first: cliente real `wp_presentation` escrito antes; baseline sem o global → `VEREDITO: FALHA`.
2. `wp_presentation` no registry e `presentation_feedback` entregue com `seq` monotônico **por output** no headless
   (n≥1 callbacks, log bench `PRESENT_FEEDBACK t=… output=… seq=… n=…`).
3. winit: feedback após `submit`; udev: feedback no vblank (`frame_submitted`), sem enviar no commit.
4. Regressões: `headless_render_damage.py`, `multi_output_damage.py`, `frame_buttons.py`, `frame_window_id.py`,
   `frame_resize_glue.py`, `frame_fixed_canvas_hygiene.py` → OK.
5. Gate: `check.sh`, clippy `--features bench -D warnings`, `--no-default-features`; DRM julgado pelo humano.
6. Nenhuma amostra descartada; sondas só sob `bench` (strings do binário normal = 0).

## Fora de escopo
- dmabuf (cartão 4), vsync/frame_painted customizados, streaming de eventos (F2), `protocols/*.xml`, ADR, deps novas,
  desempenho em VM sem 3D.

## Limites
- Tempo: 2 h sem feedback entregue → PARAR e reportar. Dependências: nenhuma.

## Entrega
Relatório curto + STATE §3/§7/§9 + commit local. **PARAR para ok visual antes da F2.**
