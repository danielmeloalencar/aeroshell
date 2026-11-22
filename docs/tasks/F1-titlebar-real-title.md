# Tarefa F1-titlebar-real-title — título real na titlebar (RASCUNHO — aguarda "ok")

> Cartão pequeno de feature (não é gate de fase). Um cartão por sessão; se o objetivo mudar, PARE e pergunte.

## Objetivo (uma frase, verificável)
Ao associar um frame e quando o título do toplevel mudar, o `#title` do HTML do `frame_webview` mostra o título
real do cliente (ex.: `foot -T aero-title-probe`), em vez do texto fixo `janela`.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-shell/examples/frame_webview.rs:64-76` (`FrameEntry`), `:533-546` (`associate_frame`),
  `:332-346` (`evaluate_javascript`), `:408-421` (`window_ipc`, que lê **uma** linha — insuficiente para `list`),
  `:434-459` (HTML/CSS com `<span id="title">janela</span>` e handler `aero`).
- `crates/aeroshell-compositor/src/window_ipc.rs:30-41` (`window_title` lê `XdgToplevelSurfaceData.title`) e
  `:80-102` (resposta `W id x y w h <titulo-json>` terminada por `.`).
- `crates/aeroshell-shell/src/bridge.rs:15-35` (`forward`: `shutdown(Write)` + ler até `.`, padrão para `list`),
  `:45-56` (`evaluate_javascript` com `window.__aeroReply(...)`).
- `docs/RUNBOOK.md:26-42` (comando nested) e `tests/integration/frame_window_id.py` (padrão de harness com 2 `foot`).

### APIs/regras confirmadas antes de editar
- IPC já expõe o título: sonda real com `--backend headless --command "foot -T aero-title-probe"` →
  `W 0 60 80 798 598 "aero-title-probe"` + `.` (saída crua registrada na sessão).
- `foot -T/--title=TITLE` define o título inicial (`foot --help`).
- `glib::timeout_add_local(Duration, callback)` existe em `glib-0.22.11/src/source.rs:455`; `ControlFlow::Break`
  encerra. `evaluate_javascript` já é usado no mesmo example (`:332`) e no bridge (`:45`) — sem dependência nova.
- Nenhuma alteração em `protocols/*.xml`, ADR ou `Cargo.toml`.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste-first:** harness novo `tests/integration/frame_title.py` (nested, `frame_webview`, `AERO_WINDOW_IPC_SOCK`,
   `foot -T aero-title-probe`). Baseline com o código atual: `VEREDITO: FALHA` — nenhuma linha
   `FRAMEWEB: title_applied` e o `#title` continua `janela`.
2. Depois da correção: o JS `aeroSetTitle` grava no `#title` e devolve por `postMessage` o **valor lido de volta do DOM**
   (`el.textContent`), que o shell loga como `FRAMEWEB: title_applied window_id=<id> titulo=<...>`. Harness →
   `VEREDITO: OK` com o título igual ao do `list` do IPC (3 execuções seguidas, amostras falhas preservadas).
3. **Mudança de título:** com um segundo cliente (`foot -T aero-title-segundo`), o frame correspondente passa a mostrar
   o título novo em até ~1 s. O IPC é request/response: usar **polling curto** (≤ 1 s) e só reenviar JS quando o título
   mudar. Se no futuro for preciso um evento de mudança de título, registrar como melhoria **F2 (streaming)** — não bloquear.
4. **Regressão:** harnesses de frame (`frame_window_id`, `frame_reopen`, `frame_buttons`, `frame_resize_glue`,
   `headless_render_damage`) verdes + `bash scripts/check.sh` verde numa única execução pós-fix.
5. **Juiz visual:** titulo real visível na barra do `foot`/`gedit`. Aplicação deixada aberta; quem fecha é o humano
   (AGENTS.md §0.9). Sem "ok", fica em `STATE.md §7` e o cartão NÃO fecha.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Geometria/9-slice, `protocols/*.xml`, ADRs, dependências novas, acionar eventos de título no protocolo.
- Mudanças no IPC do compositor (já fornece o título) ou no painel React.
- Melhoria F2 de streaming de títulos (só registrar como dívida, não implementar).

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md §6` (1–3 linhas: sintoma, onde, hipótese) e VOLTE. Não investigue desvios por mais de 15 min.

## Limites
- Tempo: até ~2 h sem resultado verificável → pare e descreva o bloqueio.
- Nenhuma dependência nova; arquivos protegidos (AGENTS.md §6) intocados.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais.
Ao final: gate colado, `STATE.md` atualizado (§3/§7/§9) e commit local (sem push). Só fechar com o "ok" visual.
