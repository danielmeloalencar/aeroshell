# Tarefa F2-decoration-policy — política de decoração em Bun (RASCUNHO — aguarda "ok")

> Segundo cartão da F2. F2-event-stream FECHADA (`02158a9`, juiz visual em `3ebed39`): o stream tipado já entrega
> estado; este cartão entrega o **mecanismo** de política (quem decide decorar ou não), não a política completa.
> F1 permanece fechada: nenhum protocolo Wayland/ADR/XML novo, nenhuma geometria/9-slice reaberto.

## Objetivo (uma frase, verificável)
O compositor expõe `app_id/título/modo/CSD` no stream e pergunta a um runtime Bun opcional como decorar cada janela;
sem runtime (ou com timeout), mantém exatamente o comportamento atual — provado por teste-first, gates e juiz visual.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-ipc/src/types.rs` — `AeroEvent` (F2-event-stream) é a fonte dos bindings TS; `WindowInfo` hoje
  não tem `app_id/csd/mode`, que já existem na linha textual do IPC (`window_ipc.rs::list`).
- `crates/aeroshell-compositor/src/window_ipc.rs` — `emit_event`, `window_command`, `list/focus/close/maximize/icon/csd`;
  adicionar **comando de política** no mesmo socket textual (sem novo protocolo Wayland).
- `protocols/aero_frame_v1.xml` — PROTEGIDO: **não alterar**; os bits `state` existentes e o caminho de
  `set_insets`/supressão de frame no render (`render/scene.rs::frame_suppressed`) são o mecanismo de aplicação.
- `crates/aeroshell-shell/examples/frame_webview.rs:900-980` — CSD hoje é detectado no shell por protocolo e aplicado
  com `aeroHideFrame` + `set_insets(0,0,0,0)`; é o "hardcoded" de onde a política começa a sair.
- `packages/runtime/src/main.ts`, `packages/runtime/src/ipc-client.ts` — ponto de entrada CLI e transporte/stream do
  runtime Bun; política completa virá em cartão posterior, aqui só o canal pergunta→resposta.
- `docs/STATE.md §3` — F1 fechou CSD flash-less, título/ícone e foot decorado; qualquer regressão aqui PARA o cartão.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste-first:** `tests/integration/f2_decoration_policy.py` + runtime mock escritos antes. Política
   `"não decorar app_id=aero-no-deco"` → baseline (sem mecanismo) dá `VEREDITO: FALHA` (comando de política
   inexistente / decisão nunca aplicada). Depois: `VEREDITO: OK` com a janela sintética sem frame.
2. Compositor publica no stream um pedido de decisão com `id/app_id/title/mode/client_frame` no ponto de map/title/
   CSD (reaproveitando os eventos já emitidos), e aceita a resposta por IPC (`policy <id> server|client|none`).
   Nenhum protocolo Wayland novo; nenhuma mudança em `protocols/*.xml`.
3. **Fallback seguro:** sem runtime conectado, ou se não houver resposta em 300 ms (configurável por env de teste),
   o compositor NÃO muda nada — CSD detection atual, moldura do `foot` e flash-less do GTK intactos. Log bench
   `POLICY t=… window_id=… decision=fallback reason=no-runtime|timeout`.
4. Com runtime, a decisão é aplicada pelo caminho existente de frame (suprimir frame para `none`; manter/insets para
   `server`; CSD/client para `client`) e logada `POLICY … decision=none|server|client fallback=false`. Sonda **só sob
   `bench`**; `strings` do binário normal → 0 ocorrências.
5. Não regredir: `f1_csd_gtk.py`, `f1_csd_sem_flash.py`, `frame_title.py`, `f1_toplevel_icon.py`, `frame_buttons.py`,
   `frame_window_id.py`, `frame_resize_glue.py`, `headless_render_damage.py` → OK (ou falha pré-existente com A/B).
6. Gates: `bash scripts/check.sh` verde; `cargo clippy --workspace --all-targets --features bench -- -D warnings`
   EXIT=0; `cargo check --workspace --no-default-features` EXIT=0; `bun run generate:types` e `bun test` verdes.
7. Juiz visual (humano): `foot` continua decorado; `gedit`/GTK4 continuam sem barra dupla nem flash; a janela da
   política `none` aparece sem moldura; controlos do frame continuam funcionando. Instância deixada aberta.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Política completa em TS (quais app_ids decorar, regras por estado) — cartão F2 seguinte.
- Reabrir F1 (geometria/9-slice/protocolos), mudar `protocols/*.xml`, ADR, dependência nova externa.
- Experimento do snap (`F7-compat-snap.md`) — só com aprovação separada, nunca intercalado com F2.
- `snapd`/slot, `/etc`, troca de VT pelo agente.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md §6` (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE. Máx. 15 min de desvio.

## Limites
- Tempo: 2 h sem mecanismo funcional → PARAR e reportar com dados; sem terceira via por conta própria.
- Dependências novas: nenhuma externa; arquivos protegidos: ver `AGENTS.md §6`.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §6, §9) e comitar localmente (sem push). **PARAR para o ok visual.**
