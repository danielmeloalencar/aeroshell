# Tarefa F1c — multi-output no winit + damage incremental + 9-slice no headless

> Uma tarefa por sessão. F1a e F1b FECHADAS. PARE para o ok do humano antes de F1d.

## Objetivo (uma frase, verificável)
F1c fecha três lacunas: (1) `--outputs N` no winit renderiza **todos** os outputs com `OutputDamageTracker`
por output; (2) damage **incremental** é provado com `age>0` (repinta só a região danificada, medido);
(3) o headless exercita o branch **9-slice** com pixels da região do frame (não clear color).

## Contexto mínimo (API confirmada na fonte fixada)
- `backend/winit.rs:146-...` — hoje renderiza só `outputs.first()` / `damage_trackers.first_mut()`
- `backend/headless.rs` — age=0 fixo; sonda 02o captura só os 5 primeiros frames (clear color)
- `render/elements.rs` — `push_frame_elements` (`RENDER_SHARED kind=placeholder` em F1b)
- `smithay-0.7.0/src/backend/renderer/damage/mod.rs:692-707` — `age=0` → full damage; `age>0` usa old_damage
- `.../damage/mod.rs:213-231` — `RenderOutputResult.damage: Option<&Vec<Rectangle>>`; `None` = skipped
- `.../renderer/element/texture.rs:533` — `TextureRenderElement::from_static_texture` (compor outputs)
- `.../renderer/mod.rs:72-100` — `ContextId::new()`; `element::Id::new()`
- `.../backend/winit/mod.rs:63,241` — winit = UM backend/janela por `init()` (não criar N event loops)

## Critério de aceite (comandos exatos + saída esperada)
1. **Teste primeiro:** `python3 tests/integration/multi_output_damage.py` → FALHA no baseline (sem linhas
   `OUTPUT_FRAME output=aero-1`), log cru salvo. Depois: `VEREDITO: OK` com `OUTPUT_FRAME` para
   **todos** os N outputs e `damage=` por output.
2. **Multi-output visual:** nested `--outputs 2` com 2 `foot`, aplicação deixada aberta; humano confirma
   que as duas janelas/frames aparecem nos dois outputs e que nada pisca ao arrastar. Sem "ok", fica em §7.
3. **Damage incremental (medido, não só ligado):** com `age=1` no alvo offscreen, provar:
   - frames estáveis → `damage=none` (render skippado);
   - quando um cliente aparece/muda → `damage=` com retângulo **menor que o output**.
   Registrar a sequência crua; nenhuma amostra descartada.
4. **9-slice no 2º backend:** headless com buffer real do WebKit → `RENDER_SHARED kind=9slice`, e a sonda
   02o captura grid com a **região do frame** diferente do clear color (comparação objetiva de células).
5. `bash scripts/check.sh` verde; `cargo clippy --workspace --all-targets --features bench -- -D warnings`
   → EXIT=0; `cargo fmt --all -- --check` → EXIT=0.
6. Regressão nested: harnesses de frame do F1a mantidos (intermitências conhecidas em §6 não são gate).

## Fora de escopo (NÃO faça)
- udev/DRM/GBM/libseat (F1d), presentation-time/dmabuf/protocolos novos (F1e+), F2/F3.
- apt, pixman, `renderer_multi`/novas features no Cargo.toml (se necessário: PARAR e pedir aprovação).
- Reabrir 02p/02q/02n ou "consertar" flakes de harness (endurecer depois, dívida em §6).
- Trocar geometria/insets/escala do 9-slice aprovado no juiz (F1a).

## Limites
- Tempo: 2 h sem gate verde → parar e descrever. APIs novas: spike curto na fonte fixada, nunca inventar.

## Entrega
`feat(winit): render de todos os outputs + damage incremental — gate F1c OK`, STATE.md atualizado
(§3/§4/§5/§6/§7/§9). Parar para o ok do humano antes de F1d.
