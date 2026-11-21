# Tarefa F1a — extrair o render do frame para módulo compartilhado

> Pré-requisito do backend udev. Uma tarefa por sessão. Se o objetivo mudar, PARE e pergunte.

## Objetivo (uma frase, verificável)
`render/elements.rs` passa a conter os tipos de elemento e a construção do frame (9-slice do canvas fixo +
placeholder sólido); `backend/winit.rs` apenas consome essa API e o comportamento observado é idêntico ao
baseline (logs bench, geometria e ordem de render).

## Contexto mínimo
- `crates/aeroshell-compositor/src/backend/winit.rs:32-51` — `FrameSlice`, aliases, `render_elements!`
- `crates/aeroshell-compositor/src/backend/winit.rs:242-568` — alvo, 9-slice (`:409-502`), placeholder (`:506-564`), logs bench
- `crates/aeroshell-compositor/src/state.rs:59-82` — `FrameGeometry` (insets/shadow/titlebar_slices)
- `crates/aeroshell-compositor/src/frame_diagnostics.rs:174-202` — `note_stretch` (sob `bench`)
- `crates/aeroshell-compositor/src/frame_titlebar_probe.rs:14-22` — `TitlebarFrame` (sob `bench`)
- ADRs 0001/0006/0007; `docs/architecture.md:23-27` (canvas fixo + 9-slice permanente)
- Fonte das APIs: `~/.cargo/registry/src/*/smithay-0.7.0/src/backend/renderer/element/utils/elements.rs:12-30`

## Critério de aceite (comandos exatos + resultado esperado)
1. Teste primeiro: `cargo test -p aeroshell-compositor render_compartilhado` → **falha antes** da criação do
   módulo (`unresolved import`), passa depois.
2. Baseline ANTES da edição (logs crus salvos, nenhuma amostra descartada): harnesses de frame do RUNBOOK →
   mesmos VEREDITOS e mesmas contagens-chave DEPOIS (`RESIZE_GLUE`, `RESIZE_SLICE`, `SLICE_GEOM n=9`,
   `SLICE_COMPOSE`, `FRAME_PLACEHOLDER`). `frame_cold_start.py` e `frame_resize_sync.py` são
   **baseline-falha em `STATE.md` §7, não gates**.
3. `bash scripts/check.sh` → `OK: scripts/check.sh passou`;
   `cargo clippy --workspace --all-targets --features bench -- -D warnings` → EXIT=0.
4. Juiz visual: nested do RUNBOOK com 2 `foot`, aplicação **deixada aberta**, humano testa e dá o "ok".

## Fora de escopo (NÃO faça)
- Mudanças de geometria, insets, escala, ordem de elementos, cores, timers ou formato de log.
- Criar `render/damage.rs`, `render/animation.rs` ou outros módulos além de `render/mod.rs`/`render/elements.rs`.
- Tocar em udev/headless, protocolos, ADRs, `Cargo.toml`, dependências ou no shell/WebView.
- Reabrir 02p/02q/02n ou re-litigar resize/cold-start.

## Limites
- Tempo: 2 h sem gate verde → parar e descrever o bloqueio.
- Dependências novas: nenhuma. Arquivos protegidos: nenhum.

## Entrega
`refactor(render): extrair elementos de frame para render/elements.rs — gate F1a OK`, com STATE.md
atualizado (§3/§4/§5/§7/§9). Parar para ok visual antes de F1b.
