# Tarefa F1-xdg-toplevel-states — fullscreen, maximize, parent e min/max (RASCUNHO — aguarda "ok")

> Cartão 3 da sequência F1-final aprovada. Abrir só depois do cartão 2 fechado.
> Decisão do humano (2026-11-22): **manter** ServerSide + CSD detection; apenas expor mais estado (app_id, título, modo)
 via IPC para a política Bun na F2. **Não** mudar arquitetura nem mover a decisão de decoração para o Rust.

## Objetivo (uma frase, verificável)
O compositor honra os pedidos `xdg_toplevel` de fullscreen (sem decoração), maximize (geometria da work area),
`set_parent` (diálogo modal sobre o pai) e `set_min_size`/`set_max_size` (limites no configure), provados por cliente
real e pelo juiz visual, sem regressão de geometria.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-compositor/src/protocols/xdg_shell.rs:83-291` — `XdgShellHandler` hoje só trata toplevel/popup/
  move/resize; `maximize_request`/`fullscreen_request` são **no-op default** no Smithay 0.7.0
  (`wayland/shell/xdg/mod.rs:1144-1157`).
- `state.rs::work_area()` + `02r` (`docs/STATE.md §3`) — maximize via IPC já calcula a área não exclusiva do output e
  `frame_geometry_for_window()`; reaproveitar a mecânica, NÃO duplicar.
- `window_ipc.rs` — expor `app_id`, título e `client_frame`/modo de decoração **como estado** para a política (F2),
  sem decidir no compositor.
- Smithay 0.7.0 fixado: `wayland/shell/xdg/mod.rs` (`ToplevelState`, `SurfaceCachedState.min_size/max_size`,
  `with_pending_state`, `send_configure`) — confirmar antes de editar.

## Critério de aceite
1. Teste-first: `examples/toplevel_states_client.rs` + `tests/integration/f1_toplevel_states.py` escritos antes;
   baseline `VEREDITO: FALHA` para os 4 estados.
2. Fullscreen: cliente pede `set_fullscreen` → superfície ocupa o output e a decoração HTML desaparece (readback DOM
   `bar=hidden border=hidden`); `unset_fullscreen` restaura decoração e geometria.
3. Maximize via protocolo: `set_maximized` usa a MESMA work area da 02r (origem do frame `(0,0)`, alvo == zona);
   `unset_maximized` restaura loc+tamanho salvos.
4. Parent: diálogo modal com `set_parent` fica acima do pai em z-order e o foco/input vão para o diálogo; destruir o pai
   fecha o filho (0 erros de protocolo).
5. Min/max: cliente com min 400×300 / max 900×700 recebe configure dentro dos limites;_compositor NÃO_ estoura os limites
   ao maximizar (recortar pela work area).
6. IPC: `list` expõe `app_id`, título e modo de decoração sem mudar o formato existente de forma incompatível
   (adicionar campos, não quebrar `frame_webview.rs`).
7. Juiz visual: humano abre gedit/diálogo no nested/tty3 e confirma fullscreen sem barra dupla, maximize e modal.
8. Gate: `check.sh`, clippy bench `-D warnings`, `--no-default-features`; vizinhos `maximize_workarea.py`,
   `frame_buttons.py`, `frame_title.py`, `f1_csd_gtk.py`, `frame_resize_glue.py` → OK.

## Fora de escopo
- Mover decisão de decoração para a política Rust/Bun agora; mudar `protocols/*.xml` ou ADR; dependências novas;
  lock/PAM/greeter; avanço de F2.
- Reabrir 02r (já FECHADA pelo juiz) — reutilizar a mecânica, não re-litigar.

## Limites
- Tempo: 2 h por estado sem resultado → PARAR e reportar. Regra das duas tentativas.

## Entrega
Relatório curto + STATE §3/§7/§9 + commit local. PARAR no juiz visual.
