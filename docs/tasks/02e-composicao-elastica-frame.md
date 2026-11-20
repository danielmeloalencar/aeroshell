# Tarefa 02e — Composição elástica do frame no compositor

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
Em todo frame renderizado, a decoração fica colada ao conteúdo já commitado (±1 px) durante resize, move, maximizar e restore, sem depender do repaint assíncrono do WebKit.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-compositor/src/backend/winit.rs:127-225` — loop de render; o frame associado é importado por `WaylandSurfaceRenderElement::from_surface` no tamanho nativo do buffer.
- `crates/aeroshell-compositor/src/state.rs:36-56` — `FrameGeometry::{extra_w, extra_h, offset}` (insets + shadow).
- `crates/aeroshell-compositor/src/grab.rs:225-265` — `RESIZE_SYNC_MOTION` sob `cfg(feature="bench")`.
- `crates/aeroshell-compositor/src/protocols/compositor.rs:115-175` — `RESIZE_SYNC_CLIENT` e `RESIZE_SYNC_FRAME` no commit.
- `tests/integration/frame_resize_sync.py` — harness de motion sintético; permanece FALHO e não é editado.
- ADRs: 0001 (Plano B leve), 0004 (frame é layer surface abaixo do cliente). ADR novo 0006 é aditivo.
- APIs fixadas (Smithay 0.7.0): `render_elements!`, `RescaleRenderElement::from_element`, `WaylandSurfaceRenderElement::buffer_size()`. Padrão copiado de `anvil v0.7.0 anvil/src/render.rs`.

## Critério de aceite (comandos exatos + resultado esperado)
1. `python3 tests/integration/frame_resize_glue.py` (escrito ANTES, visto falhando no baseline): durante o grab, toda linha `RESIZE_GLUE` satisfaz `target == content + extras` e `rescaled == (buffer != target)`; após soltar + ≤500 ms existe linha com `rescaled=false` e `buffer==target`. Rodar 3× seguidas, nenhuma amostra descartada.
2. Vizinhos: `python3 tests/integration/frame_resize_edges.py`, `frame_follows_resize.py`, `frame_buttons.py`, `frame_window_id.py` → todos OK. `frame_resize_sync.py` (métrica antiga) segue FALHO, registrado.
3. `bash scripts/check.sh` → `OK: scripts/check.sh passou`; `cargo clippy --workspace --all-targets --features bench -- -D warnings` → EXIT=0.
4. Teste manual (humano): instância aberta com 2 `foot`; arrastar borda direita e canto inferior-direito, lento e rápido; critério: borda da decoração COLADA ao conteúdo, sem correr atrás. Agente deixa aberto e pede o "ok"; quem fecha é o humano (AGENTS.md §0.9).
5. ADR `docs/adr/0006-frame-resize-composicao-elastica.md` criado com causa raiz (pipeline WebKit), decisão, alternativas preteridas e gatilhos de reavaliação.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Shell, protocolo (`protocols/*.xml`), Cargo.lock, rust-toolchain.toml, ADRs aceitos.
- 02f (higiene de configures), 02g (fd no shell), 02h (medições VM), 02i (docs), 02c/02d (não editar teste nem critério antigo).
- 9-slice, viewporter, maximize-origem, blur, dependências novas.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md §6` (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem resultado visível → pare e descreva o bloqueio.
- Dependências novas: NENHUMA.
- Arquivos protegidos: ver AGENTS.md §6; ADR novo 0006 é aditivo e permitido.

## Entrega (relatório curto)
O que funciona · comandos exatos de reprodução · números crus · o que quebrou/não foi testado · STATE.md §3/§4/§6/§7/§9 atualizado · commit pequeno (sem push).
