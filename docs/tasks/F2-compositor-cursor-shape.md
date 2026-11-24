# Tarefa F2-compositor-cursor-shape — RASCUNHO (aguarda "ok" do humano)

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md
> e este arquivo. Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
O compositor aplica a forma de cursor pedida pelo cliente (`wp_cursor_shape_v1` / `wl_pointer.set_cursor`)
no backend nested, e a nossa decoração SSD pede `move` na titlebar e `resize` nas bordas — com o
juiz visual confirmando seta dupla na borda de app CSD e mão/mover na titlebar.

## Contexto mínimo (arquivos que importam) — lido nesta sessão, não suposição
- `crates/aeroshell-compositor/src/protocols/seat.rs:49-67` — `SeatHandler::cursor_image` **já grava**
  `self.cursor_image` (não é mais no-op); sonda `CURSOR_SHAPE` sob `bench`.
- `crates/aeroshell-compositor/src/state.rs:255-268` — `cursor_shape_state` (global `wp_cursor_shape_v1`,
  criado em `:564`), `cursor_image`, `cursor_images` (tema XCursor com hotspot).
- `crates/aeroshell-compositor/src/backend/winit.rs:435-441` e `:826-832` — **causa raiz no nested**:
  `window().set_cursor(if state.cursor_over_interactive { Pointer } else { Default })` ignora
  completamente `state.cursor_image`. O pedido do cliente é recebido e descartado no caminho visual.
- `crates/aeroshell-compositor/src/input.rs:43-46` e `:213-216` — `cursor_over_interactive` é só um
  `bool` de "layer surface interativa"; não distingue borda de resize de titlebar de move.
- `crates/aeroshell-compositor/src/render/cursor.rs:133-225` — `cursor_element` já existe: `Named` →
  tema XCursor (`Kind::Cursor`), `Surface` → `WaylandSurfaceRenderElement`.
- `crates/aeroshell-compositor/src/backend/udev.rs:453` — `cursor_element` é chamado **só no udev**;
  o winit nunca renderiza o cursor do cliente, apenas troca o cursor da janela host.
- `crates/aeroshell-shell/src/input_region.rs:35-40,43-64` — `rects.cursor` é lido e logado, mas
  **nunca enviado ao seat** como pedido de cursor.
- `crates/aeroshell-shell/examples/frame_webview.rs:1024-1048` — `#bar data-aero-interactive` sem
  `data-aero-cursor`; `#border` tem `pointer-events:none`; botões só `cursor:pointer` em CSS.
- STATE.md §10 registra "cursor-shape = dívida F1 sanada (sem código novo)" — o global e o handler
  existem de fato; a lacuna agora é **aplicação** (nested + SSD), não anúncio do protocolo.
- ADRs: nenhum muda. Nenhum XML de protocolo muda.

## Critério de aceite (comandos exatos + saída esperada)
1. **Teste-first (pré-filtro executável nesta VM):** escrever `examples/cursor_shape_client.rs`
   (cliente sintético que chama `wp_cursor_shape_v1.set_shape(...)` e/ou `wl_pointer.set_cursor`)
   e `tests/integration/f2_compositor_cursor_shape.py` ANTES da correção. Com o nested do RUNBOOK
   e binário `--features bench`, o harness mede a sonda do compositor no ponto em que o cursor é
   aplicado: `CURSOR_APPLY t=… shape=ew-resize icon=… source=named`.
   Baseline no binário do `HEAD` → `VEREDITO: FALHA` (sem `CURSOR_APPLY` com a forma pedida; o log
   só mostra o cursor fixo `Default`/`Pointer`). Depois da correção → **3× `VEREDITO: OK`**,
   cobrindo `ew-resize`, `ns-resize`, `move`/`grab` e `pointer`, 0 erros de protocolo.
2. **SSD:** o mesmo harness exige sonda `FRAMEWEB: cursor shape=…` quando o ponteiro entra na
   titlebar/borda da moldura HTML, e `CURSOR_APPLY` correspondente no compositor.
   Baseline → `VEREDITO: FALHA` (nenhum pedido de cursor vem do shell). Depois → `VEREDITO: OK`.
3. **Juiz visual (sentença final, §0.9):** com o nested do RUNBOOK aberto e **deixado aberto**:
   - app CSD (`gedit`): pairar sobre a borda vertical → seta dupla horizontal; borda horizontal →
     seta dupla vertical; pairar sobre a titlebar desenhada pelo GTK → forma que o GTK pedir
     (mover/mão), sem cursor preso em seta.
   - app SSD (`foot` com moldura HTML): pairar sobre a titlebar → mão/mover; pairar sobre as bordas
     de 6 px → setas duplas correspondentes; ao sair, voltar ao cursor padrão.
   O humano dá o **"ok"**. Sem ok, fica em `STATE.md` §7 como não validado e o cartão NÃO fecha.
   O agente não encerra a aplicação.
4. **Gates:** `bash scripts/check.sh` EXIT=0; `cargo clippy --workspace --all-targets --features bench
   -- -D warnings` EXIT=0; `cargo check --workspace --no-default-features` EXIT=0; sonda `CURSOR_APPLY`
   com `strings` no binário sem `bench` → 0 ocorrências.
5. **Vizinhos:** `frame_buttons.py`, `frame_resize_edges.py`, `f1_csd_gtk.py`,
   `headless_render_damage.py`, `frame_title.py`, `f2_shell_real_policy_restore.py` → OK.
   Qualquer amostra falha é impressa e registrada em §6, nunca descartada em silêncio.
6. **Commits:** um por unidade lógica (compositor/nested; shell/SSD; instrumentação se precisar),
   gate verde em cada um. Sem push.

## Abordagem proposta (padrão que eu escolheria; aviso se discordar)
- **Compositor/nested:** derivar o cursor da janela host de `state.cursor_image`:
  `Named(icon)` → `winit::window::CursorIcon` mapeado por `icon.name()` (já existem os nomes em
  `render/cursor.rs:57-77`); `Surface(s)` → interpretar o hotspot e, se viável no mesmo passo,
  renderizar a superfície na cena com `Kind::Cursor`; `Hidden` → ícone oculto. Manter
  `cursor_over_interactive` só como fallback quando não há pedido do cliente.
- **Shell/SSD:** o `#bar` passa a declarar `data-aero-cursor="grab"` (mover) e as zonas de borda
  declaram `ew-resize`/`ns-resize`/cantos; `input_region::apply` envia a forma ao seat via
  `wp_cursor_shape_v1` (ou `wl_pointer.set_cursor` se o global não existir), sem mudar o input region.
- **udev:** já tem `cursor_element`; só confirmar que não regrediu (juiz no tty3 fica FORA deste
  cartão, salvo decisão contrária do humano).

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Reabrir `F2-shell-real` (dívida de pintura aceita), snap/F7, F1, lock/PAM/greeter, política de
  decoração, XML de protocolos, ADR, temas de cursor novos, desempenho em VM sem 3D.
- Trocar o backend udev ou mexer em geometria/9-slice/grabs de move-resize.
- Tema Xcursor novo ou dependência externa: `xcursor` já está no workspace; nada novo.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Regra das duas tentativas (AGENTS.md §0.4).

## Limites
- Tempo: se em 2 h não houver veredito automatizado estável + instância pronta para o juiz,
  PARAR e reportar com dados — não tentar terceira via por conta própria.
- Dependências novas: nenhuma. Arquivos protegidos: ver AGENTS.md §6. Sem sudo/VT/`/etc`.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado ·
gate colado · confirmação visual do humano (OK/pendente) · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §6, §7, §9) e comitar localmente (sem push).
