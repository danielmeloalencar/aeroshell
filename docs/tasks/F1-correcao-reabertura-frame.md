# Cartão de correção F1 — foot reaberto perde a decoração (RASCUNHO — aguarda "ok")

> **BUG, não fase.** F1f está FECHADA; F1g tem cartão próprio (`F1g-dmabuf-presentation.md`) e **não é
> executado nesta sessão**. Um cartão por sessão. Pare para o ok visual do humano antes de qualquer próximo.

## Objetivo (uma frase, verificável)
Abrir → fechar → reabrir um `foot` no nested resulta na decoração HTML funcional na nova janela, com
`window_id` novo e sem reaproveitamento de ID.

## Contexto mínimo — causas CONFIRMADAS no código
1. **`FrameEvent::Closed` só loga.** `crates/aeroshell-shell/examples/frame_webview.rs:324-327`:
   `state.closed += 1; eprintln!("FRAMEWEB: closed recebido");` — **não remove o `FrameEntry`**
   (`FrameEntry` em `:54-66`) nem destrói a `gtk4::ApplicationWindow`. Assim o novo `frame_requested`
   com o mesmo `window_id` encontra a entrada antiga e cai em
   `FRAMEWEB: window_id=… já apresentado; ignorado` (`:256`), sem `present()`.
2. **`window_id` é reutilizado.** `crates/aeroshell-compositor/src/protocols/xdg_shell.rs:52`:
   `let n = self.space.elements().count() as i32;` — quando o `foot` fecha, a janela sai do `Space`, o
   `count()` cai e o próximo cliente recebe o **mesmo id**. Isso contraria a promessa de identidade
   estável do `aero_frame_v1.xml` (`:25-26,55-57`: associação com janela fechada é corrida legítima,
   não reabertura do mesmo id).
3. **Não é o proveito dos avisos.** `xdg_activation`, `text_input`, `xdg_toplevel_icon` e `hangup`
   são protocolos faltantes (F1i/F7) — registrar separadamente em `STATE.md §6`, **fora deste cartão**.

## Correção (escopo exato)
- **Shell:** no `FrameEvent::Closed`, remover o `FrameEntry` correspondente ao `frame` recebido
  (`frame: &AeroFrameV1` já está dispunível no `Dispatch`, `:277`), chamar `window.destroy()` na
  `ApplicationWindow` e descartar pedidos pendentes daquele `window_id` na fila do pool. Log bench:
  `FRAMEWEB: closed window_id=… entradas_restantes=…`.
- **Compositor:** campo `next_window_id: u32` em `AeroState`, iniciado em 0; `new_toplevel` passa a
  alocar `id = next_window_id` e incrementar (saturando em `u32::MAX` — nunca reutilizar). Substituir
  `n` por `id` em `:52-83` (posição em cascata, `window_surfaces.insert`, `frame_requested`).
  `window_count` em `aero_frame.rs:304` continua sendo contagem de janelas **vivas** (detecção de
  corrida), não o contador monotônico.
- **Sem tocar** em `protocols/*.xml`, ADR, geometria/9-slice/escala, lock/PAM, F1g.

## Teste-primeiro (visto falhando antes da correção)
Harness novo `tests/integration/frame_reopen.py`: nested com `--shell frame_webview`, abrir `foot`,
fechar aquela janela pelo `--close <id>` (IPC já existente), abrir outro `foot` e exigir:
- novo `window_id` (maior que o anterior),
- `frame_requested` do novo id **não** seguido de `já apresentado; ignorado`,
- `associate` + `FRAME_PAINTED`/`SLICE_COMPOSE` para o novo id (decoração presente).

Baseline esperado: `window_id` repetido e `já apresentado; ignorado` → `VEREDITO: FALHA`.

## Critério de aceite
1. Harness `frame_reopen.py` gravado antes, falhando no baseline e verde depois (saída crua colada).
2. Regressão: `frame_window_id.py`, `frame_follows_resize.py`, `frame_buttons.py` → OK.
3. `bash scripts/check.sh` verde (uma execução); clippy bench `-D warnings` EXIT=0;
   `cargo check --workspace --no-default-features` EXIT=0.
4. Juiz visual do humano no tty3: abrir→fechar→reabrir `foot` e confirmar decoração na nova janela.

## Como usar o domicile — guia, NÃO receita
Invariante transferível: **identidade de janela é estável e cada elemento é keyed pelo id**
(`ONE-PAGE-FOR-THE-DESK.md:152-154`: um float que troca de tela mantém o elemento, então o `<webview>`
não recarrega); uma janela fechada vira registro explícito em aposentadoria, nunca um id reciclado
(`WINDOW-MANAGEMENT.md:495-501`). Não copiar animação/fade/Chromium. Fonte executável: nosso código +
protocolo fixado.

## Fora de escopo
- Avisos `xdg_activation`/`text_input`/`xdg_toplevel_icon`/`hangup` (F1i/F7) — só §6.
- F1g (dmabuf/presentation-time), subsurfaces, lock/PAM, ADR/XML/dep, sudo/VT/push.

## Se encontrar OUTRO problema
Anotar 1–3 linhas em `STATE.md §6` e VOLTAR; máx. 15 min de desvio.

## Limites
- Tempo: 2 h sem harness verde → PARAR e reportar com dados.
- Dependências: nenhuma.

## Entrega
Relatório curto + `STATE.md §3/§6/§7/§9` + commit local. PARAR antes do ok visual e de qualquer próximo cartão.
