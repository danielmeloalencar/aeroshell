# Tarefa F1-csd-gtk-move-resize — apps GTK/CSD sem barra dupla e com move/resize (EM EXECUÇÃO — aguarda reteste visual)

> **2 juízes visuais NEGARAM (2026-11-22); 3ª causa encontrada por medição:** humano confirmou move/resize OK, mas
> nossa barra ainda aparece nos apps GNOME. O gedit é GTK3 (`ldd`) e `libgdk-3.so.0` usa
> `org_kde_kwin_server_decoration_manager` — protocolo que faltava (o Domicile tem `KdeDecorationHandler`).
> Implementado global KDE + sonda `reason=kde`. Aguardando 3º julgamento no tty3.

> Decisão do humano (2026-11-22): cartão **único** para os dois sintomas observados no `gedit` (tty3): barra de
> decoração duplicada e move/resize que não funcionam. Um cartão por sessão; se o objetivo mudar, PARE e pergunte.

## Objetivo (uma frase, verificável)
Um toplevel que pede decoração `ClientSide` (GTK4/gedit) não recebe a titlebar HTML do AeroShell e passa a ser
movido/redimensionado pelos pedidos padrão `xdg_toplevel.move`/`resize`, sem quebrar o frame dos clientes atuais.

## Contexto mínimo (fontes confirmadas)
### O que o AeroShell faz hoje
- `crates/aeroshell-compositor/src/protocols/xdg_shell.rs:182-199` — `XdgDecorationHandler` ignora o `mode` pedido e
  responde sempre `ServerSide`; o comentário assume que clientes CSD "largam a própria barra" (vale para `foot`, não
  para GTK4).
- `xdg_shell.rs:130-146` — `move_request` e `resize_request` são **no-op** ("grabs de move/resize entram em F3").
- `crates/aeroshell-compositor/src/grab.rs:425-475` — `start_geometry_grab(state, window_id, serial, resize)` já
  implementa `MoveGrab`/`ResizeGrab` com validação de serial (`aero_frame::serial_valid`) e é usado só pelo frame HTML.
- `input.rs:78` guarda `last_button_serial` também para presses no cliente — é a base para validar o serial do
  `xdg_toplevel.move/resize`.
- `render/scene.rs:132,151` e `backend/winit.rs:465` já compensam `window.geometry().loc` no render (sombra CSD).

### Guia do projeto de referência (invariantes, não receita)
- `/home/daniel/Projetos/domicile-engine/packages/domicile-compositor/src/decorations.rs:40-70,93-98` — responde
  `ServerSide`, mas grava `AsksToDrawItsFrame` no `SurfaceData.data_map` quando o cliente pede `ClientSide`.
- `.../windows.rs:124` publica `clientFrame`; `.../manganese-window-management/src/window-changes.ts:112`,
  `window-state.ts:492`, `floating/float.ts:153`, `placement.ts:198` — o shell **não desenha titlebar** para float CSD
  ("bare"); documentado em `docs/WRITING-A-SHELL.md:437-439`.
- `.../window_geometry.rs` recorta sombras CSD e `.../client_requests.rs:316` compensa o offset do ponteiro.
- Smithay 0.7.0 fixado: `wayland/shell/xdg/decoration.rs:163` (`request_mode`), `wayland/compositor/mod.rs:165-173`
  (`SurfaceData.data_map`), `UserMap::insert_if_missing_threadsafe` (usado em `desktop/wayland/popup/manager.rs:146`).

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste-first:** cliente de teste (example, sem dep nova) cria toplevel, pede
   `zxdg_toplevel_decoration_v1.set_mode(ClientSide)`, e chama `xdg_toplevel.move`/`resize` com serial de botão válido.
   Harness novo `tests/integration/f1_csd_gtk.py` com input sintético. Baseline: `VEREDITO: FALHA` — titlebar HTML
   ainda visível (`FRAMEWEB: csd_applied ... bar=visible` ausente) e geometria `W 0 …` inalterada após move/resize.
2. **CSD detectado:** gravar `clientFrame` no `data_map` da superfície e expor ao shell por **novo comando IPC**
   `csd <id>` → `CSD <id> 1|0`. **NÃO alterar o formato de `list`** (`packages/sdk/src/index.ts:12-25` faz
   `JSON.parse` do restante da linha). Sonda bench `CSD_DETECT window_id=<id> client_frame=true`.
3. **Sem barra dupla:** ao associar um frame CSD, o shell esconde `#bar`/botões (probe DOM lida de volta via
   `postMessage`: `FRAMEWEB: csd_applied window_id=<id> client_frame=true bar=hidden`) e envia insets sem a faixa de
   título. Render/input continuam alinhados com `window.geometry()` (mesma compensação já usada no render).
4. **Move/resize:** implementar `move_request`/`resize_request` em `xdg_shell.rs` usando `start_geometry_grab`, com o
   serial validado (`serial_valid(last_button_serial, serial)`). Harness mede geometria antes/depois do move e do
   resize → tamanho/posição mudam; serial inválido é recusado (sem grab).
5. **Regressão:** clientes atuais (foot, que NÃO pede CSD) continuam com a barra HTML, `frame_buttons`,
   `frame_resize_edges`, `frame_follows_resize`, `frame_title`, `f1_toplevel_icon` e `bash scripts/check.sh` verdes
   numa única execução pós-fix.
6. **Juiz visual:** no tty3/nested com `gedit` real, **uma** barra de título (a do GTK) e move/resize funcionando.
   Aplicação deixada aberta; quem fecha é o humano (AGENTS.md §0.9). Sem "ok", fica em `STATE.md §7`.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Nautilus / `org.freedesktop.Tracker3.Miner.Files` — é spike D-Bus separado, decidido à parte.
- `protocols/*.xml`, ADRs, dependências novas, mudança em `list`, temas/DPI alto, decoração de popup.
- Reabrir ícone/título (fechados e aprovados), geometria/9-slice dos clientes que NÃO pedem CSD.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md §6` (1–3 linhas: sintoma, onde, hipótese) e VOLTE. Máx. 15 min de desvio.

## Limites
- Tempo: ~2 h. Se não houver critério verificável, PARE e relate (regra das duas tentativas).
- Nenhuma dependência nova; arquivos protegidos (AGENTS.md §6) intocados.

## Entrega
O que funciona · como testar à mão · o que NÃO foi testado · gate colado · STATE atualizado · commit local (sem push).
Só fechar com o "ok" visual.
