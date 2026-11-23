# Tarefa F1-final-1b — diagnóstico de menus Qt (subsurfaces/popup) (APROVADA 2026-11-23)

> Cartão de diagnóstico aberto após o juiz visual negar o cartão 1 (F1-subsurfaces). Um cartão por sessão;
> PARE para ok visual antes do cartão 2 (clipboard).

## Objetivo (uma frase, verificável)
Identificar, com log debug e comparação contra cliente sintético que funciona, por que itens de menu do `qterminal`
(Qt) não respondem ao clique e tooltips não aparecem, e aplicar a correção mínima — com juiz visual positivo.

## Contexto mínimo
- Juiz (2026-11-23): *"Tooltips não aparecem… clique em arquivo/editar/help abre o menu, mas itens dentro não funcionam"*.
- Pré-filtro provado: subsurfaces sintéticas 3× OK (pixels + hit-test); popup sintético com `grab()` recebe 48 botões
  em headless **e** winit. Logo o mecanismo genérico funciona; o defeito é específico do Qt real.
- `protocols/xdg_shell.rs`: `new_popup`, `reposition_request`, `unconstrain_popup`, `grab`, `handle_commit`.
- `protocols/compositor.rs`: `commit` (input_region, buffer).
- `state.rs::surface_under` (layers → frames → `Window::surface_under(...ALL)`).
- `input.rs`: `process_input_event` (`motion` → `surface_under`, `button` → `focus_on_press` + `pointer.button`).
- Smithay 0.7.0 fixado: `wayland/shell/xdg/mod.rs` (`PositionerState`), `desktop/wayland/popup/grab.rs`
  (`PopupPointerGrab::button`): no `Pressed`, se `current_focus` não é do cliente do popup, o grab é dispensado.

## Abordagem (log + comparação; NÃO mexer no mecanismo)
1. Instrumentar SOMENTE sob `cfg(feature = "bench")`: positioner recebido (âncora, gravity, anchor_rect, size, offset),
   geometria ANTES/DEPOIS de `unconstrain_popup`, `input_region` e buffer do popup no commit, foco/enter/leave e
   estado do grab no clique.
2. Rodar nested (winit) com `qterminal` e `RUST_LOG=debug` (o `~/u`/tty3 é operado pelo humano; agente não troca VT).
3. Capturar log com menus operados pelo humano; comparar com o cliente sintético que funciona.
4. Hipóteses a confirmar, uma por vez: (a) `unconstrain_popup` altera geometria; (b) foco não transfere para o popup
   antes do `button press` → `PopupPointerGrab` dispensa o grab; (c) `input_region` ausente; (d) Qt usa `wl_subsurface`
   em vez de `xdg_popup` e nosso hit-test não o trata como popup.

## Critério de aceite
1. Log debug cru mostrando o que o Qt envia de diferente do sintético, com `arquivo:linha` da causa.
2. Correção mínima (uma coisa por vez), teste-first visto falhando, gate verde.
3. Regressão: `tests/integration/f1_subsurfaces.py` e `tests/integration/f1_popup_click.py` → `VEREDITO: OK`
   (headless + winit).
4. Gate: `bash scripts/check.sh`, clippy `--features bench -D warnings`, `--no-default-features`.
5. Juiz visual: humano abre qterminal, clica itens de Arquivo/Editar/Ajuda e vê tooltips; dá o "ok".

## Fora de escopo
- Mudar o mecanismo de subsurfaces (já provado), protocolos além de popup/subsurface, cartão 2 (clipboard), F2.
- Reabrir cartões fechados; avançar de fase; dependências novas; `protocols/*.xml`; ADR.

## Limites
- Tempo: 2 h sem causa identificada → PARAR e reportar com dados. Regra das duas tentativas (§4.1).
- Sondas só sob `bench`; nenhuma amostra descartada.

## Entrega
Relatório curto + STATE §3/§6/§7/§9 + commit local. PARAR no ok visual.
