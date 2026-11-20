# Tarefa 02c — A decoração acompanha o conteúdo durante o resize

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
Ao redimensionar uma janela com o mouse, cada novo tamanho do conteúdo tem a decoração (frame WebView) commitada
no tamanho correspondente antes do próximo passo de motion, com defasagem p95 ≤ 300 ms — sem a barra "correr atrás".

## Contexto mínimo (arquivos que importam)
- Relato do humano (2026-11-20): "o conteúdo da janela acompanha o mouse muito bem, porém a decoração fica atrasada
  correndo atrás". É o juiz humano, não uma suspeita teórica; 02b foi fechada com esta pendência explícita.
- `crates/aeroshell-compositor/src/protocols/compositor.rs:125-151` — o frame só recebe `send_frame_configure`
  no COMMIT do cliente, acrescentando um ciclo entre conteúdo e decoração.
- `crates/aeroshell-compositor/src/protocols/aero_frame.rs:59-100` — `send_frame_configure`: configure duplo
  (`aero_frame.configure` com o conteúdo + `zwlr_layer_surface.configure` com o total).
- `crates/aeroshell-shell/examples/frame_webview.rs:241-260` — handler de `FrameEvent::Configure` chama
  `window.set_default_size(total)` e `frame.ack_configure(serial)`.
- `crates/aeroshell-shell/examples/frame_webview.rs:458-461` — `glib::timeout_add_local` de 20 ms despacha os
  eventos Wayland do guest (hipótese de origem da defasagem; MEDIR antes de afirmar).
- `crates/aeroshell-compositor/src/backend/winit.rs:321-327` — log `S5 ... latency_ms` mede só o fim do grab
  até o render; NÃO mede a defasagem entre conteúdo e decoração. Não usar como prova deste critério.
- `tests/integration/frame_resize_edges.py` — infraestrutura de motion sintético + leitura de log (modelo).
- ADR 0001 (frame: Plano B com buffer sobredimensionado + src-rect) e ADR 0004 (associate só marca a superfície).

## Critério de aceite (comandos exatos + resultado esperado)
1. Instrumentação de medição (pode ficar sob `#[cfg(feature = "bench")]`): registrar, por passo de resize,
   `t_cliente` (commit do conteúdo W×H) e `t_frame` (commit da layer surface com buffer W+insets × H+insets),
   e derivar `lag_ms = t_frame - t_cliente`.
   `python3 tests/integration/frame_resize_sync.py` → `VEREDITO: OK`, com `lag p95 ≤ 300 ms` e
   `passos_com_frame_pendente_no_proximo_motion = 0`, em 3 execuções seguidas (nenhuma amostra descartada).
2. Teste manual (juiz humano): executar o nested do RUNBOOK (`docs/RUNBOOK.md §21`) com 2 `foot`, arrastar uma
   borda/canto em movimentos contínuos e conferir que a barra e as bordas ficam COLADAS ao conteúdo, sem correr
   atrás. Deixar a aplicação aberta; o humano dá o "ok" e fecha (`AGENTS.md §0.9`).
3. `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0, gate completo numa única execução).

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Não alterar `protocols/*.xml`, ADRs aceitos, `rust-toolchain.toml` nem `Cargo.lock` (AGENTS.md §6).
- Não trocar WebKitGTK 6.0 / gtk4-layer-shell por outra estratégia (atlas, WPE) sem ADR novo aprovado.
- Não reabrir 02b (window_id, foco, hit-test), nem maximizar/origem do output.
- Não medir desempenho em GPU real: ambiente é VM sem 3D; vale só funcionamento e defasagem RELATIVA.
- Não desligar verificação para esconder erro (`--release`, `#[allow]`, comentar teste).

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem melhoria mensurável no lag → pare, escreva o que tentou e peça decisão. Se a mesma abordagem
  falhar 2 vezes, PARE (regra das duas tentativas, AGENTS.md §4.1).
- Dependências novas: nenhuma sem aprovação (AGENTS.md §3.10).
- Arquivos protegidos: ver AGENTS.md §6 — NENHUMA mudança sem aprovação explícita.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar `docs/STATE.md` (§3, §4, §5, §9) e comitar localmente (sem push).

## Resultado (2026-11-20) — PARCIAL, BLOQUEADA
- Critério 1: NÃO PASSA. `python3 tests/integration/frame_resize_sync.py` → `VEREDITO: FALHA`, com
  `lag p95=127,19 ms` (≤ 300 ms, OK) mas `13/14` commits de conteúdo com a decoração pendente no próximo motion
  (exigido: 0). Teste escrito antes da correção e visto falhando: baseline `p95=177,22 ms / 13-14 pendentes`.
- Critério 2 (visual): NÃO EXECUTADO. Sem o critério 1 verde, não abri a instância para pedir o "ok" do humano.
- Critério 3: PASSA. `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0). Clippy extra com
  `--features bench` também verde. Vizinhos: `frame_window_id`, `frame_buttons`, `frame_resize_edges`,
  `frame_follows_resize` → 4× `VEREDITO: OK`.
- Correção mantida: `send_frame_configure` no `ResizeGrab::motion` (`grab.rs`), em paralelo com o configure do
  cliente, em vez de esperar o commit (`compositor.rs:140-149`). Reduziu o lag ~28%.
- Abordagem 2 tentada e REVERTIDA: timer de despacho do shell de 20 → 1 ms piorou o p95 (127,19 → 158,05 ms)
  sem reduzir a pendência (13/14). Amostra crua em `/tmp/opencode/sync-after3.log`.
- Limitação medida: com a decoração em WebView, o commit do frame leva ~60–100 ms; motions de 50 ms tornam
  "zero pendentes" inatingível sem tirar o WebKit do caminho crítico (Plano B / ADR 0001).
- Duas abordagens falharam no critério → pare conforme `AGENTS.md §3.12` (terceira troca proibida). Decisão do
  humano pendente: (A) aprovar Plano B com ADR novo, (B) redefinir critério para p95 ≤ 300 ms e fechar,
  (C) manter parcial e abrir cartão novo.
