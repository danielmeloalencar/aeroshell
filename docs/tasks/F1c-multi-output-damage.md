# Tarefa F1c — multi-output no winit + damage incremental (REIMPLEMENTAÇÃO)

> RASCUNHO aguardando aprovação do humano. Uma tarefa por sessão. Se o objetivo mudar, PARE e pergunte.
> Pré-requisitos: F1a (`342552d`) e F1b (`740b98d`) FECHADAS. PARE para o ok do humano antes de F1d.
> Reimplementação nova sobre o estado pós-`4307cc4`; **NÃO** restaurar o código antigo via
> `git revert 4307cc4`/`git checkout` dos commits revertidos.

## Objetivo (uma frase, verificável)
F1c fecha três lacunas: (1) `--outputs N` no winit renderiza **todos** os outputs com `OutputDamageTracker`
por output; (2) damage **incremental** é provado com `age>0` (frames estáveis sem repaint, dano parcial medido);
(3) o headless exercita o branch **9-slice** com verificação objetiva de pixels na região do frame.

## Contexto mínimo (fonte confirmada, não inventada)
- `crates/aeroshell-compositor/src/backend/winit.rs:125-128` — hoje só `outputs.first()`/`damage_trackers.first_mut()`;
  `:290` `render_output(..., 0, ...)` (age=0); `:334-335` `submit` com o damage
- `crates/aeroshell-compositor/src/backend/headless.rs` — age=0 fixo; sonda 02o após `render_output`
- `crates/aeroshell-compositor/src/render/elements.rs` — `push_frame_elements` (F1a; não mudar geometria/logs)
- `crates/aeroshell-compositor/src/cli.rs:34,104-110` — `--outputs N` já existe (default 1)
- `~/.cargo/.../smithay-0.7.0/src/backend/renderer/damage/mod.rs:692-707` — `age=0` → full damage;
  `age>0` com `old_damage` suficiente → reusa dano anterior
- `.../damage/mod.rs:213-231` — `RenderOutputResult.damage: Option<&Vec<Rectangle>>`; `None` = skipped
- `.../renderer/element/texture.rs:533` — `TextureRenderElement::from_static_texture` (compor outputs)
- `.../backend/winit/mod.rs:63` — winit = UM backend/janela por `init()` (não criar N event loops)

## Hipótese de desenho (validar com spike antes de codar; NÃO é fato)
Com N>1, renderizar cada output num alvo offscreen próprio (textura/renderbuffer) com seu próprio
`OutputDamageTracker` e compor as texturas na janela host via `TextureRenderElement`; o caminho N=1 deve
ficar **inalterado** (regressão F0/F1a/F1b). Se o spike mostrar caminho mais simples na fonte fixada, usar
o mais simples e registrar a escolha. Se exigir feature nova no Cargo.toml ou apt: **PARAR e pedir aprovação**.

## Critério de aceite (comandos exatos + saída esperada)
1. **Teste primeiro:** `python3 tests/integration/multi_output_damage.py` → FALHA no baseline (sem linhas
   `OUTPUT_FRAME output=aero-1`), log cru salvo. Depois: `VEREDITO: OK` com `OUTPUT_FRAME` para **todos**
   os N outputs e `damage=` por output.
2. **Multi-output visual:** nested `--outputs 2` com 2 `foot`, aplicação deixada aberta; humano confirma que
   as duas janelas/frames aparecem nos dois outputs e que nada pisca ao arrastar. Sem "ok", fica em §7.
3. **Damage incremental (medido, não só ligado):** com `age>0`, provar na sequência crua:
   frames estáveis → `damage=none` (render skippado); quando um cliente aparece/muda → `damage=` com
   retângulo **menor que o output**. Nenhuma amostra descartada; se o ambiente impedir, medir e registrar.
4. **9-slice no 2º backend:** headless com buffer real do WebKit → `RENDER_SHARED kind=frame` presente, e a
   sonda 02o mostra a **região do frame** diferente do clear color (comparação objetiva de células do grid,
   não só "bytes não nulos").
5. `bash scripts/check.sh` verde; `cargo clippy --workspace --all-targets --features bench -- -D warnings`
   → EXIT=0; `cargo fmt --all -- --check` → EXIT=0.
6. Regressão nested: harnesses de frame de F1a/F1b mantidos (intermitências conhecidas em §6 não são gate;
   `frame_cold_start.py`/`frame_resize_sync.py` seguem baseline-falha em §7).

## Fora de escopo (NÃO faça)
- udev/DRM/GBM/libseat (F1d), presentation-time/dmabuf/protocolos novos (F1e+), F2/F3.
- apt, pixman, `renderer_multi`/novas features no Cargo.toml ou `Cargo.lock` (se necessário: PARAR e pedir).
- Reabrir 02p/02q/02n ou "consertar" flakes de harness (dívida em §6).
- Trocar geometria/insets/escala/clear color do 9-slice aprovado no juiz (F1a) ou os logs do render compartilhado.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas) e VOLTE. Máximo 15 minutos; só pare tudo se bloquear o aceite.

## Limites
- Tempo: 2 h sem gate verde → parar e descrever. APIs novas: spike curto na fonte fixada, nunca inventar.
- Arquivos protegidos: nenhum. Dependências novas: nenhuma.

## Entrega (relatório curto)
`feat(winit): render de todos os outputs + damage incremental — gate F1c OK`, STATE.md atualizado
(§3/§4/§5/§6/§7/§9). O que funciona · comandos de reprodução · o que NÃO foi medido · juiz visual pendente
ou OK. **Parar para o ok do humano antes de F1d.**
