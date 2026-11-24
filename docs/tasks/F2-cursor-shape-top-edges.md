# Tarefa F2-cursor-shape-top-edges — FECHADO PELO JUIZ VISUAL (2026-11-24: "aprovado")

> Cartão de correção de seguimento de `F2-compositor-cursor-shape` (fechado, mas o juiz encontrou
> um sintoma residual). Uma tarefa por sessão; se o objetivo mudar, PARE e pergunte.

> **Veredito (2026-11-24):** humano operou a instância `wayland-aero-f2-cursor-top` (foot SSD e
> gedit CSD, topo/cantos superiores e titlebar) e respondeu *"aprovado"*. Cartão fechado; gate final
> `bash scripts/check.sh` EXIT=0, harness `f2_compositor_cursor_shape.py` 3× OK com 6 pontos SSD.
> A instância foi fechada pelo humano, não pelo agente.

## Objetivo (uma frase, verificável)
Na moldura SSD, a faixa superior de 6 px e os dois cantos superiores mostram, respectivamente,
`ns-resize`, `nwse-resize` e `nesw-resize` — sem mudar o cursor do resto da titlebar (`move`) nem
o comportamento de clique (que já inicia os grabs `Top`/`TopLeft`/`TopRight`).

## Contexto mínimo
- `crates/aeroshell-shell/examples/frame_webview.rs:1024-1044` — `#bar` (`top:0;height:30px;cursor:all-scroll`)
  cobre os 6 px superiores; `#edge-tl/#edge-tr` vêm **antes** de `#bar` no DOM e não têm `z-index`;
  **não existe `#edge-t`**. Por isso o WebKit aplica `move` no topo, em vez do cursor de resize.
- O handler de `button` já detecta `on_top` e cantos com `BORDER=6` e chama `frame.resize(...)`
  com `Top`, `TopLeft`, `TopRight` — ou seja, o clone é resize, só o cursor está errado.
- CSD (`gedit`, sem moldura HTML) já está 100% aprovado pelo humano: **não mexer** no compositor.
- Nenhum protocolo/ADR/dependência nova; `wp_cursor_shape_v1` e o `apply_host_cursor` já funcionam.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste-first:** estender `tests/integration/f2_compositor_cursor_shape.py` com três pontos na
   moldura: topo (`x+w/2, y-27`), canto superior esquerdo (`x-3, y-27`) e superior direito
   (`x+w+3, y-27`). No binário atual → `VEREDITO: FALHA` (sem `CURSOR_SHAPE`/`CURSOR_APPLY` com
   `ns-resize`, `nwse-resize`, `nesw-resize` nesses pontos; amostra crua colada no relatório).
2. Depois da correção → **3× `VEREDITO: OK`**, com os 5 casos anteriores (`wait`, `text`, `move`,
   `ns-resize` na borda inferior) ainda passando.
3. **Juiz visual:** humano usa a instância deixada aberta e confirma que o topo/cantos superiores
   agora mostram setas duplas diagonais/vertical, e que o resto da titlebar continua `move`.
   Sem "ok", item fica em `STATE.md` §7 como não validado.
4. `bash scripts/check.sh` EXIT=0; clippy `--features bench -D warnings` 0; vizinhos `frame_buttons`,
   `frame_resize_edges`, `f1_csd_gtk`, `frame_title` OK.

## Fora de escopo
- Compositor, protocolo/ADR, apps CSD, política de decoração, F2-shell-real, snap/F7, desempenho.

## Limites
- Tempo: 1 h sem veredito automatizado estável → PARAR e reportar. Sem dependências novas,
  sem sudo/VT, sem arquivos protegidos. Regra das duas tentativas (AGENTS.md §0.4).

## Entrega
Relatório curto: o que funciona · como testar à mão · gate colado · confirmação visual do humano ·
descobertas laterais. Atualizar `STATE.md` e commitar localmente (sem push).
