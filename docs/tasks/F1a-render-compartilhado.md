# Tarefa F1a — extrair o render do frame para módulo compartilhado (REIMPLEMENTAÇÃO)

> RASCUNHO aguardando aprovação do humano. Uma tarefa por sessão. Se o objetivo mudar, PARE e pergunte.
> Reimplementação do zero sobre o estado de F0 fechado (`4a2ccb2`), após `4307cc4 revert(f1)`.
> **NÃO** reintroduzir o código antigo com `git revert 4307cc4` nem `git checkout` dos commits revertidos:
> a implementação é nova, com teste-primeiro e prova própria.

## Objetivo (uma frase, verificável)
`render/elements.rs` passa a conter os tipos de elemento e a construção do frame (9-slice do canvas fixo +
placeholder sólido); `backend/winit.rs` apenas consome essa API e o comportamento observado é idêntico ao
baseline (logs bench, geometria e ordem de render).

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-compositor/src/backend/winit.rs:32-51` — `FrameSlice`, `SliceRect`/`SlicePoint`, `render_elements!`
- `crates/aeroshell-compositor/src/backend/winit.rs:64-712` — montagem da cena, 9-slice, placeholder e logs bench
- `crates/aeroshell-compositor/src/state.rs:59-82` — `FrameGeometry` (insets/shadow/titlebar_slices)
- `crates/aeroshell-compositor/src/frame_diagnostics.rs` — `note_stretch` (sob `bench`)
- `crates/aeroshell-compositor/src/frame_titlebar_probe.rs:14-22` — `TitlebarFrame` (sob `bench`)
- `crates/aeroshell-compositor/src/main.rs:3-22` — árvore de módulos (hoje sem `render`)
- ADRs 0001/0006/0007; `docs/architecture.md` (canvas fixo + 9-slice permanente)
- Fonte das APIs Smithay: `~/.cargo/registry/src/*/smithay-0.7.0/src/backend/renderer/element/utils/elements.rs`

## Critério de aceite (comandos exatos + resultado esperado)
1. Teste primeiro: `cargo test -p aeroshell-compositor render_compartilhado` → **falha antes** da criação do
   módulo (`unresolved import`/`cannot find render`), log cru salvo; passa depois.
2. Baseline ANTES da edição (logs crus salvos, nenhuma amostra descartada): harnesses de frame do RUNBOOK →
   mesmos VEREDITOS e mesmas contagens-chave DEPOIS (`RESIZE_GLUE`, `RESIZE_SLICE`, `SLICE_GEOM n=9`,
   `SLICE_COMPOSE`, `FRAME_PLACEHOLDER`). `frame_cold_start.py` e `frame_resize_sync.py` são
   **baseline-falha em `STATE.md` §7, não gates**.
3. `bash scripts/check.sh` → `OK: scripts/check.sh passou`;
   `cargo clippy --workspace --all-targets --features bench -- -D warnings` → EXIT=0;
   `cargo fmt --all -- --check` → EXIT=0.
4. Juiz visual: nested do RUNBOOK com 2 `foot`, aplicação **deixada aberta**, humano testa e dá o "ok"
   (barras/bordas idênticas, barra da frente sobre conteúdo da de trás, resize lento/rápido, botões).

## Fora de escopo (NÃO faça)
- Mudanças de geometria, insets, escala, ordem de elementos, cores, timers ou formato de log.
- Criar `render/damage.rs`, `render/animation.rs`, backend headless/udev ou qualquer módulo além de
  `render/mod.rs`/`render/elements.rs` (F1b/F1c/F1d são cartões futuros, com aprovação própria).
- Tocar em protocolos, ADRs, `Cargo.toml`, `Cargo.lock`, dependências ou no shell/WebView.
- Reabrir 02p/02q/02n ou re-litigar resize/cold-start; corrigir corridas de parse dos harnesses (dívida §6).

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa. Não investigue desvios
por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem gate verde → parar e descrever o bloqueio.
- Dependências novas: nenhuma. Arquivos protegidos: nenhum.

## Entrega (relatório curto)
`refactor(render): extrair elementos de frame para render/elements.rs — gate F1a OK`, com STATE.md
atualizado (§3/§4/§5/§6/§7/§9). O que funciona · como testar à mão · o que não foi testado · descobertas
laterais. **Parar para ok visual do humano antes de F1b.**
