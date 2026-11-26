# F2-windows-completo — contrato `aero.windows` e maximize multi-output

> Cartão fornecido e autorizado pelo humano em 2026-11-25. Base: `69054ab`
> (F2-outputs+input-binds fechado). Uma tarefa por sessão; PARAR para o juiz visual.
> FECHADO em 2026-11-26: integração, 15 vizinhos, gates e 3 perfs verdes; juiz final
> **“aprovado com dívida de otimizacao futura”**. Somente `Geometry` release-only foi
> mantido como otimização; redraw por output e skip de hit-test foram revertidos.

## Objetivo (uma frase, verificável)

Completar `aero.windows` com estados/eventos reativos e operações IPC+SDK, fazendo
maximize/fullscreen usarem a work area do output sob a janela, descontando as
exclusive zones desse output.

## Contexto mínimo (arquivos que importam)

- `docs/plano-mestre.md` §3 (`aero.windows`) — contrato público da F2.
- `crates/aeroshell-compositor/src/state.rs` — `work_area()` usa `outputs.first()`.
- `crates/aeroshell-compositor/src/protocols/toplevel_states.rs` — maximize/fullscreen.
- `crates/aeroshell-compositor/src/grab.rs`, `input.rs` — grabs reais de move/resize.
- `crates/aeroshell-compositor/src/window_ipc.rs` — IPC privilegiado e stream PUSH.
- `crates/aeroshell-ipc/src/types.rs` — única fonte dos tipos públicos via ts-rs.
- `packages/sdk/src/{index.ts,store.ts,api/}` — API `aero.windows` e stores reativos.
- `examples/blank-desktop/` — JSDoc/exemplo de uso; `defineDesktop/rules` permanecem.
- `tests/integration/f2_windows_completo.py` — baseline e prova ponta a ponta a criar.

## Escopo

1. Maximize/fullscreen escolhem o output pela localização/centro da janela e usam a
   work area daquele output, descontando exclusive zones; corrigir `work_area()`.
2. `aero.windows`: minimize (esconde sem perder membership/workspace), maximize,
   fullscreen, move, resize, beginMove e beginResize (grab no Rust; IPC+SDK).
3. Eventos `Minimized`, `Maximized`, `Fullscreen` e `Geometry` no stream, gerados por
   ts-rs, com stores reativos.
4. Thumbnail fica DEFERIDO para screencopy/F7 e retorna erro explícito.
5. `defineDesktop/rules` seguem; adicionar JSDoc e exemplo conforme a regra 6.

## Critério de aceite (comandos exatos + resultado esperado)

1. **Baseline antes da implementação:** testes novos e
   `python3 tests/integration/f2_windows_completo.py` falham por maximize no output 0
   e/ou API/comandos ausentes; saída crua preservada em `/tmp/opencode`.
2. **Depois:** `python3 tests/integration/f2_windows_completo.py` → `VEREDITO: OK`,
   com dois outputs: janela no output 1 maximiza para a work area do output 1;
   minimize esconde sem perder workspace; fullscreen cobre o output; beginMove e
   beginResize funcionam com ponteiro injetado; eventos e store acompanham o estado.
3. **Gates:** `bash scripts/check.sh` verde;
   `cargo clippy --workspace --all-targets --features bench -- -D warnings` exit 0;
   `cargo check --workspace --no-default-features` exit 0.
4. **Vizinhos:** frame/policy/hotreload/workspaces/launcher/host/sdk/hmr/outputs-binds
   → `VEREDITO: OK`; `python3 tests/integration/f2_surface_perf.py` mantém 1 WebProcess
   no host e 0 redraws estáticos.
5. **Juiz visual:** multi-output maximize/minimize/fullscreen + drag/resize via SDK e
   60 s responsivo; aplicação aberta e aprovação explícita do humano são obrigatórias.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)

- Thumbnail/screencopy (F7), além do erro explícito de deferimento.
- Layouts, tiling, animações e overview (F3).
- Idle/lock (F5), adapters, snap/F7.
- Hotplug udev, IDs estáveis de outputs e correção da colisão de porta HMR.
- Correção de `frame_follows_resize` (pré-existente).
- Novas dependências sem aprovação e qualquer `git push`.

## Dívidas a preservar/registrar

- Conflito de porta HMR (fail-fast/porta única) permanece dívida separada.
- Hotplug/IDs estáveis permanecem fora deste recorte.
- `frame_follows_resize` permanece pré-existente; não corrigir neste cartão.

## Limites

- Tempo: cerca de 2 h sem resultado verificável → pare e descreva o bloqueio.
- Dependências novas: nenhuma.
- Arquivos protegidos e protocolos Wayland: não alterar sem aprovação explícita.
- Regra das duas tentativas e teste-first conforme `AGENTS.md`.

## Entrega (relatório curto)

Resultado · comandos e saídas crus · juiz visual · não executado/não medido ·
descobertas laterais · próximo passo proposto sem executá-lo. Atualizar `STATE.md`
(§3, §4, §5, §6, §7, §9), commit local somente com autorização e sem push.

Depois, somente com cartões próprios: alpha-confirm → adapters React/Solid/Vue +
exemplos (Three.js) → fechamento F2.
