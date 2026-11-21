# Tarefa F1b — backend headless com render e damage

> Uma tarefa por sessão. Pré-requisito: F1a FECHADA (`render/elements.rs`). PARE para ok antes de F1c.

## Objetivo (uma frase, verificável)
O backend headless passa a renderizar com `GlesRenderer` EGL surfaceless + `OutputDamageTracker` por output,
reutilizando `render/elements.rs` (9-slice/placeholder) num 2º backend, com verificação de pixels pela
sonda 02o — prova honesta de protocolo/lógica/damage na VM sem 3D.

## Contexto mínimo (fonte confirmada, não inventada)
- `crates/aeroshell-compositor/src/backend/headless.rs:26-61` — hoje só cria output, sem render
- `crates/aeroshell-compositor/src/render/elements.rs` — `push_frame_elements`, `AeroRenderElements` (F1a)
- `backend/winit.rs:146-...` — referência de montagem da cena + `render_output` + `send_frame`
- `crates/aeroshell-compositor/src/frame_pixel_probe.rs:31-125` — sonda 02o (`ExportMem`, sob `bench`)
- `~/.cargo/.../smithay-0.7.0/Cargo.toml:81-88` — `backend_winit` já traz `backend_egl` e `renderer_gl`
- `~/.cargo/.../smithay-0.7.0/src/backend/egl/native.rs:262-275` — `EGLSurfacelessDisplay`
- `.../src/backend/renderer/gles/mod.rs:468` — `GlesRenderer::new` (unsafe, exige `// SAFETY:`)
- `.../src/backend/renderer/gles/mod.rs:1517` — `Bind<GlesRenderbuffer>`; `:1596` — `Offscreen<GlesRenderbuffer>`
- `~/.cargo/.../calloop-0.14.5/src/sources/timer.rs:63,180` — `Timer::from_duration`, `TimeoutAction`

## Critério de aceite (comandos exatos + resultado esperado)
1. Teste primeiro: `python3 tests/integration/headless_render_damage.py` → FALHA no baseline (sem linhas
   `HEADLESS_FRAME`/`PIXEL_FRAME`), log cru salvo. Depois: `VEREDITO: OK` com ≥3 `HEADLESS_FRAME`,
   ≥1 `PIXEL_FRAME`, grid de pixels 64×40×3 com conteúdo não nulo e campo `damage=` presente.
2. Prova do render compartilhado: linhas `RENDER_SHARED window_id=<id> kind=<frame|placeholder>` no
   headless, emitidas pelo mesmo `push_frame_elements` usado no winit.
3. Damage por output: `HEADLESS_FRAME output=<nome> damage=<rect|none>`; primeiro frame com damage,
   frames seguintes sem mudança com `damage=none` (se o ambiente não permitir, medir e registrar).
4. `bash scripts/check.sh` verde; `cargo clippy --workspace --all-targets --features bench -- -D warnings`
   → EXIT=0; `cargo fmt --all -- --check` → EXIT=0.
5. Regressão nested: harnesses de frame do F1a continuam com os mesmos VEREDITOS (§3 do STATE).
   `frame_cold_start.py`/`frame_resize_sync.py` seguem baseline-falha em §7, não gates.
6. Nenhum `unsafe` sem `// SAFETY:`; nenhuma dependência, feature do Cargo, protocolo ou ADR novo.

## Fora de escopo (NÃO faça)
- udev/DRM/GBM/libseat (F1d), multi-output real, presentation-time/dmabuf/protocolos novos (F1e+).
- Pixman, `renderer_pixman`, apt, `Cargo.toml`, `Cargo.lock`, `protocols/*.xml`, ADRs.
- Trocar o clear color, a geometria do 9-slice ou os logs do winit.
- Reabrir 02p/02q/02n; corrigir as corridas de parse dos harnesses (dívida em §6).

## Limites
- Tempo: 2 h sem gate verde → parar e descrever o bloqueio. Se precisar de pixman/apt/feature nova: PARAR e pedir aprovação.
- Arquivos protegidos: nenhum.

## Entrega
`feat(headless): render EGL surfaceless + damage com render/elements.rs compartilhado — gate F1b OK`,
STATE.md §3/§4/§5/§6/§7/§9 atualizado. Parar para ok visual do humano antes de F1c.
