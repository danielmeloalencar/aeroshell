# Tarefa 02s (RASCUNHO — aguardando "ok" para executar) — duas instâncias não podem roubar a decoração uma da outra

> Cartão novo, aberto a pedido do humano em 2026-11-22 após fechar a 02r. Dívida registrada em
> `STATE.md` §6: ao rodar `~/u` no tty3 com a instância nested ainda aberta, o `frame_webview` novo
> morria no startup e a janela aparecia **sem decoração**; ao fechar a nested e repetir, o juiz aprovou.

## Objetivo (uma frase, verificável)
Com duas instâncias AeroShell vivas no mesmo barramento D-Bus de sessão, cada `frame_webview` executa o
seu `connect_activate`, conecta como shell privilegiado do **seu** compositor e pinta a decoração — nenhum
processo novo apenas reativa a instância antiga e sai sem log.

## Contexto mínimo
- `crates/aeroshell-shell/examples/frame_webview.rs:803-844` — `main()` cria o `gtk4::Application` com
  `application_id("dev.aeroshell.frame-webview")` **fixo** (linha 806). Se outro processo já possui esse
  nome no D-Bus, o `app.run()` encaminha a ativação para ele e o novo processo termina sem executar o
  `connect_activate` → zero linhas `FRAMEWEB`, nenhum `associate`, nenhuma decoração.
- `crates/aeroshell-compositor/src/state.rs:588-594` — `ClientData::disconnected` loga
  `conexão privilegiada do shell caiu reason=ConnectionClosed` quando isso acontece.
- APIs **já conferidas na fonte fixada** (nada inventado):
  - `gtk4-0.11.5/src/auto/application.rs:72` — `ApplicationBuilder::flags(gio::ApplicationFlags)`;
  - `gio-0.22.11/src/auto/flags.rs:112-113` — `ApplicationFlags::NON_UNIQUE` (`G_APPLICATION_NON_UNIQUE`).
  - Alternativa, se o teste mostrar que `NON_UNIQUE` não basta: sufixar o `application_id` com um
    identificador da sessão (nome do socket ou PID), higienizando os caracteres permitidos em nomes D-Bus.
- Evidência crua da 02r: `/home/daniel/aeroshell-udev-20261122-183642.log` e
  `/home/daniel/aeroshell-udev-20261122-184400.log` (0 `FRAMEWEB`, shell cai ~40 ms após o launch),
  `/tmp/opencode/02r/probe-normal-ipc.log` (0 `FRAMEWEB`, 1 queda),
  `/tmp/opencode/02r/probe-dbus.log` (20 `FRAMEWEB`, 0 quedas com `dbus-run-session`).
- `docs/RUNBOOK.md` — comando nested e `~/u` no tty3.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste primeiro:** criar `tests/integration/frame_two_instances.py` e rodar no código atual →
   `VEREDITO: FALHA`. O harness lança o compositor A (nested, socket próprio, D-Bus do host) com
   `--shell frame_webview`, espera `FRAMEWEB: manager global` em A; depois lança o compositor B
   (nested, socket próprio, **mesmo D-Bus**) e verifica que B **também** chega a `FRAMEWEB: manager global`
   e a `frame_painted`, com `W 0 ...` no IPC de B. Baseline esperado: B com `FRAMEWEB=0` e
   `conexão privilegiada do shell caiu`, A saudável. Preservar a amostra em `/tmp/opencode/02s/`.
2. Depois da correção, o MESMO comando 3× seguidas, sem descartar amostras → `VEREDITO: OK`:
   `FRAMEWEB: manager global` e `frame_painted` em **A e B**, `W 0 ...` no IPC de ambos, **zero**
   `conexão privilegiada do shell caiu` em ambos. Nenhum processo iniciado pelo harness sobra.
3. **Juiz visual obrigatório:** com a instância nested **aberta de propósito**, humano roda `~/u` no
   tty3 e confirma que a decoração aparece e que ✕/maximizar respondem. Aplicação deixada aberta;
   quem fecha é o humano (AGENTS.md §0.9). Sem o "ok", a tarefa NÃO é concluída.
4. Vizinhos: `frame_buttons.py`, `frame_reopen.py`, `frame_window_id.py`, `frame_resize_glue.py` → OK.
   A proteção `ACTIVATED` (`frame_webview.rs:815-818`) contra `activate` repetido **dentro da mesma
   instância** deve continuar funcionando (não remover esse guard).
5. `bash scripts/check.sh` → `OK: scripts/check.sh passou`;
   `cargo clippy --workspace --all-targets --features bench -- -D warnings` → 0;
   `cargo check --workspace --no-default-features` → 0.

## Fora de escopo
- Maximizar/work area, insets, 9-slice, canvas, ADR 0007 — 02r fechada, não reabrir.
- Trocar protocolo (`protocols/*.xml`), ADRs, painel React, dependências novas, desempenho em VM.
- Matar processos por nome: o harness encerra **somente** os PIDs que ele iniciou.

## Se você encontrar OUTRO problema
Anotar em `STATE.md` §6 (1–3 linhas: sintoma, onde, hipótese) e VOLTAR. Só investigar se bloquear o
aceite, e por no máximo 15 min. Regra das duas tentativas: após 2 falhas da mesma abordagem, parar.

## Limites
- Tempo: 2 h sem resultado visível → parar e descrever o bloqueio.
- Uma correção por vez, teste primeiro. Sem `sleep`/constantes duplicadas, sem `allow`/`unwrap` novos.
  Sem mexer em arquivos protegidos. Se exigir ADR, **parar e perguntar**.

## Entrega
Resultado · teste manual exato · confirmação visual · provas cruas antes/depois · não executado/não medido ·
descobertas laterais (já em §6) · próximo passo sem executar. Atualizar `STATE.md` §§3–7 e §9;
commit local sem push.
