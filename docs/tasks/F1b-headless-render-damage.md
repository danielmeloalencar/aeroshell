# Tarefa F1b — backend headless com render e damage (REIMPLEMENTAÇÃO)

> RASCUNHO aguardando aprovação do humano. Uma tarefa por sessão. Se o objetivo mudar, PARE e pergunte.
> Pré-requisito: F1a FECHADA (`342552d` + `99c8296`, `render/elements.rs`). PARE para ok antes de F1c.
> Reimplementação nova sobre o estado pós-`4307cc4`; **NÃO** restaurar o headless antigo via
> `git revert 4307cc4`/`git checkout` dos commits revertidos.

## Objetivo (uma frase, verificável)
O backend headless passa a renderizar com `GlesRenderer` EGL surfaceless + `OutputDamageTracker` por output,
reutilizando `render/elements.rs` (9-slice/placeholder) num 2º backend, com verificação de pixels pela
sonda 02o — prova honesta de protocolo/lógica/damage na VM sem 3D.

## Contexto mínimo (fonte confirmada, não inventada)
- `crates/aeroshell-compositor/src/backend/headless.rs:26-61` — hoje só cria output, sem render nem timer
- `crates/aeroshell-compositor/src/render/elements.rs` — `push_frame_elements`, `AeroRenderElements`,
  `FrameRenderInput`, `FrameRenderResult` (F1a; NÃO mudar seus logs nem sua geometria)
- `crates/aeroshell-compositor/src/backend/winit.rs` — referência de montagem da cena + `render_output` + `send_frame`
- `crates/aeroshell-compositor/src/frame_pixel_probe.rs:31-45` — sonda 02o (`ExportMem`, grade 64×40, sob `bench`)
- `~/.cargo/.../smithay-0.7.0/Cargo.toml` [features] — `backend_winit` já traz `backend_egl` + `renderer_gl`
- `~/.cargo/.../smithay-0.7.0/src/backend/egl/native.rs:262-275` — `EGLSurfacelessDisplay`
- `~/.cargo/.../smithay-0.7.0/src/backend/egl/display.rs:201` — `EGLDisplay::new` (unsafe, exige `// SAFETY:`)
- `~/.cargo/.../smithay-0.7.0/src/backend/egl/context.rs:119` — `EGLContext::new(&display)`
- `~/.cargo/.../smithay-0.7.0/src/backend/renderer/gles/mod.rs:468` — `GlesRenderer::new` (unsafe)
- `~/.cargo/.../smithay-0.7.0/src/backend/renderer/gles/mod.rs:1517,1599` — `Bind`/`Offscreen<GlesRenderbuffer>`
- `~/.cargo/.../calloop-0.14.5/src/sources/timer.rs:63,180` — `Timer::from_duration`, `TimeoutAction`

## Critério de aceite (comandos exatos + resultado esperado)
1. Teste primeiro: `python3 tests/integration/headless_render_damage.py` → FALHA no baseline (sem linhas
   `HEADLESS_FRAME`/`PIXEL_FRAME`), log cru salvo. Depois: `VEREDITO: OK` com ≥3 `HEADLESS_FRAME`,
   ≥1 `PIXEL_FRAME`, grid de pixels 64×40×3 com conteúdo não nulo e campo `damage=` presente.
2. Prova do render compartilhado: o headless emite `RENDER_SHARED window_id=<id> kind=<frame|placeholder>`
   a partir do `FrameRenderResult.kind` devolvido pelo **mesmo** `push_frame_elements` do winit
   (log do backend headless; não adicionar logs ao módulo compartilhado nem ao winit).
3. Damage por output: `HEADLESS_FRAME output=<nome> damage=<rect|none>`; primeiro frame com damage.
   Se frames estáveis não chegarem a `damage=none` (alvo offscreen sem buffer-age), **medir e registrar**,
   sem otimizar e sem claim de redução de damage.
4. `bash scripts/check.sh` verde; `cargo clippy --workspace --all-targets --features bench -- -D warnings`
   → EXIT=0; `cargo fmt --all -- --check` → EXIT=0.
5. Regressão nested: harnesses de frame do F1a mantêm os VEREDITOS do baseline de F1a (intermitências já
   registradas em §6 não são gate). `frame_cold_start.py`/`frame_resize_sync.py` seguem baseline-falha em §7.
6. Nenhum `unsafe` sem `// SAFETY:`; nenhuma dependência, feature do Cargo, protocolo ou ADR novo.

## Fora de escopo (NÃO faça)
- udev/DRM/GBM/libseat (F1d), multi-output real, damage incremental por `age>0` (F1c),
  presentation-time/dmabuf/protocolos novos (F1e+), F2/F3.
- Pixman, `renderer_pixman`, apt, `Cargo.toml`, `Cargo.lock`, `protocols/*.xml`, ADRs.
- Trocar o clear color, a geometria do 9-slice ou os logs do winit/render compartilhado.
- Reabrir 02p/02q/02n; corrigir as corridas de parse dos harnesses (dívida em §6).

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas) e VOLTE. Máximo 15 minutos; só pare tudo se bloquear o aceite.

## Limites
- Tempo: 2 h sem gate verde → parar e descrever o bloqueio.
- **Se precisar de pixman, apt ou feature nova no Cargo.toml: PARAR e pedir aprovação** (não contornar).
- Arquivos protegidos: nenhum.

## Entrega (relatório curto)
`feat(headless): render EGL surfaceless + damage com render/elements.rs compartilhado — gate F1b OK`,
STATE.md §3/§4/§5/§6/§7/§9 atualizado. O que funciona · comandos de reprodução · o que NÃO foi medido.
Juiz visual: headless não tem tela — a prova é por logs/pixels; se o winit for tocado, abrir nested e pedir
ok visual. **Parar para ok do humano antes de F1c.**
