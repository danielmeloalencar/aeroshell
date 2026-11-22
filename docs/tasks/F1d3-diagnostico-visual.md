# Tarefa F1d3-diagnóstico — tela sem imagem no DRM (juiz negou em 2026-11-22)

> Cartão de diagnóstico (continuação da F1d3, não fecha a F1d3). Uma tarefa por sessão.
> Pré-requisito: F1d3 implementada; humano rodou `~/run-aeroshell-udev.sh` no tty3 e negou.

## Objetivo (uma frase, verificável)
Produzir evidência objetiva — no log da próxima execução no tty3 — de que a sessão está ativa,
o CRTC está ativo, o ciclo `render_frame`→`queue_frame`→vblank submitted acontece (ou mostrar
exatamente onde para), e devolver ao humano o atalho `Ctrl+Alt+F1..F4` para trocar de VT.

## Contexto mínimo
- `/home/daniel/aeroshell-udev-20261122-023701.log` — amostra crua do juiz: heartbeat até `tick=1500`,
  nenhum `render_frame falhou`/`queue_frame falhou`, 3 grabs de move/resize, foco de teclado no `foot`,
  nenhuma linha de evento de sessão, fim do log com NULs binários.
- `crates/aeroshell-compositor/src/backend/udev.rs:77-80,92-102,193-210,228-270,287-322` — pontos
  de observação e único lugar onde o libinput é ligado.
- `crates/aeroshell-compositor/src/input.rs:140-152` — teclado é só repassado ao seat; `grep -rn change_vt crates/`
  → 0 ocorrências (o RUNBOOK promete `Ctrl+Alt+F2`, mas o compositor nunca chama `change_vt`).
- Smithay 0.7.0 fixado: `Session::is_active()` (`backend/session/mod.rs:41`), `DrmDevice::is_active()`
  (`backend/drm/device/mod.rs:449`), `RenderFrameResult::{is_empty,needs_sync}` (`backend/drm/compositor/
  frame_result.rs:40-62`), `DrmOutput::{render_frame,queue_frame,frame_submitted}` (`backend/drm/output.rs:589-648`),
  `KeyboardKeyEvent::{key_code,state}` (`backend/input/mod.rs:100-114`), `Session::change_vt` (`backend/session/mod.rs:41`).
- **Não usar** a ausência do WARN "drm master" como causa: `backend/drm/device/fd.rs:74-79` diz explicitamente
  que kernels novos permitem modeset sem ele.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste primeiro:** `python3 tests/integration/udev_frame_probe.py /home/daniel/aeroshell-udev-20261122-023701.log`
   → `EXIT=1`, `VEREDITO: FALHA`, lista de linhas ausentes (amostra crua salva).
   `python3 tests/integration/udev_frame_probe.py --self-test` → `EXIT=0` (parser provado com fixture sintética;
   **não** é evidência de produto).
2. Instrumentação em `udev.rs`: `UDEV_SESSION`, `UDEV_OUTPUT`, `UDEV_FRAME`, `UDEV_VBLANK`,
   `UDEV_SESSION_EVENT` e `UDEV_VT` em `tracing` (sem feature nova, sem `unsafe`/`unwrap`/`allow`).
   `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo check --workspace --no-default-features` → `EXIT=0`.
3. `bash scripts/check.sh` → `OK: scripts/check.sh passou` numa única execução;
   `bash tests/integration/s1_nested.sh`, `python3 tests/integration/headless_render_damage.py`,
   `dbus-run-session -- python3 tests/integration/multi_output_damage.py` → verdes (regressão).
4. **Juiz:** humano reexecuta no tty3 com `RUST_LOG=debug`, usa `Ctrl+Alt+F2` e volta com `Ctrl+Alt+F3`,
   e envia o log. `udev_frame_probe.py <log>` → `VEREDITO: OK` com `session active=true`,
   `output active=true`, `frame queued=ok`, `vblank submitted=ok`. O veredito visual final (imagem na tela)
   continua sendo o humano — este teste **só** prova que o caminho de apresentação foi exercitado.
5. Com essa evidência, escolher UMA hipótese e corrigir em cartão separado, com sua aprovação.

## Fora de escopo (NÃO faça)
- Corrigir a imagem "no escuro" (sem o log instrumentado do humano, AGENTS §0.3).
- Cursor visível, hotplug, multi-GPU, plano B (dumb+legacy+pixman), lock/PAM, novas dependências,
  protocolos/ADRs/arquivos protegidos.
- Mudar geometria, ordem/cores 9-slice, cadência do render (16 ms) ou formato de log existente.
- Encerrar aplicação ou trocar VT pelo agente — humano faz as duas coisas (AGENTS §0.9).

## Se você encontrar OUTRO problema
Anotar em `docs/STATE.md` §6 (1–3 linhas) e VOLTAR. Limite de 15 min por desvio.

## Limites
- Tempo: ~2 h; se a causa não aparecer no log instrumentado, PARAR e pedir decisão.
- Regra das duas tentativas: não trocar de abordagem 3× sem decisão do humano.
- Sem `sudo`, `/etc` ou pacotes; sem `git push`.

## Entrega
O que foi medido (comando → saída crua) · o que **não** foi medido (pixels da tela) ·
STATE.md §3/§4/§5/§6/§7/§9 atualizado · commit local · PARAR para o humano reexecutar.
