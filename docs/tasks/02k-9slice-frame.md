# Tarefa 02k — 9-slice do frame (gatilho do ADR 0006 disparado pelo juiz)

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.
> Autorização do humano recebida em 2026-11-20: o juiz reprovou a escala uniforme ("flicker geral,
> bordas; não natural como Windows/GNOME"). Isto executa o **gatilho 1 já previsto no ADR 0006:100-103**,
> não uma família nova.

## Objetivo (uma frase, verificável)
Durante o arrasto de resize, a espessura das bordas (6 px) e a altura da titlebar (30 px) da decoração
permanecem constantes em todo frame renderizado — sem pulsação/snap da escala —, provado por medição
objetiva nova e pelo juiz visual, sem mudar a geometria `target = content + extras`.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-compositor/src/backend/winit.rs` — `AeroRenderElements` e o `RescaleRenderElement`
  uniforme da 02e; é aqui que entra o 9-slice. Fonte das APIs: Smithay 0.7.0 fixado
  (`backend/renderer/element/utils/elements.rs`), padrão `anvil v0.7.0 anvil/src/render.rs`.
- `crates/aeroshell-compositor/src/grab.rs` — configures no grab e o debounce de 200 ms da 02f;
  a tarefa congela o buffer no grab (um configure grande no início, um final no unset).
- `crates/aeroshell-compositor/src/state.rs:36-56` — `FrameGeometry::{extra_w, extra_h}`; a geometria
  JÁ está correta (`docs/STATE.md` §6) e **não muda**.
- `tests/integration/frame_resize_glue.py` — mede geometria/escala por frame; vai ganhar a medição de
  espessura/altura constante e de ausência de snap.
- Evidência do juiz: flicker geral no teste manual (PID 84904, `/tmp/opencode/aero-02j-visual.log`),
  6 episódios `FRAME_STRETCH_ACTIVE` (até 5439 ms, `max_scale` 2,165) e 1
  `FRAME_NEVER_DREW_AT_SIZE` com `buffer=964x530` vs alvo `708x530`. Na 02f: `max_scale` 6,839 e
  `|buffer-target|` p95 434 px. Causa hipotetizada: escala uniforme de bordas/titlebar + debounce.
- ADR 0006 é **proposto/aditivo**, não aceito: não editar; se a decisão mudar, escrever ADR novo.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste primeiro, visto falhando:** estender `tests/integration/frame_resize_glue.py` para assertar,
   em todo frame do arrasto, que a espessura das bordas e a altura da titlebar renderizadas são iguais
   aos insets (6/30 px) e que não há snap periódico da escala das fatias. Rodar e salvar baseline FALHA
   (borda engrossa com a escala), sem descartar amostra.
2. 9-slice no render do frame com `fgeo.insets` como linhas de corte: 4 cantos sem escala
   (`CropRenderElement` + `RelocateRenderElement`); left/right crop + `Rescale` só em Y; bottom e
   titlebar crop + `Rescale` só em X; centro crop + `Rescale` em ambos. Novas variantes em
   `render_elements! AeroRenderElements`; coordenadas via `buffer_size()`/`view()`.
3. Congelar buffer no grab: UM `send_frame_configure` com tamanho grande no início; nenhum configure
   durante o grab (remover o debounce de 200 ms **desse caminho**); UM configure final no unset com
   `client_content_size()`. Commit-hook continua calado durante o grab (`resize_grab_active`).
4. Gate: teste novo 3× OK; vizinhos (`frame_configure_hygiene.py`, `frame_resize_edges.py`,
   `frame_follows_resize.py`, `frame_buttons.py`, `frame_window_id.py`, `frame_resize_glue.py`) OK;
   `frame_resize_sync.py` permanece FALHA; `bash scripts/check.sh` verde;
   `cargo clippy --workspace --all-targets --features bench -- -D warnings` EXIT=0.
5. **JUIZ VISUAL (humano):** 2 `foot`, arrasto lento e rápido das bordas e da barra → bordas coladas,
   **espessura constante**, sem piscar. Sem o "ok", item fica em STATE.md §7 e a tarefa NÃO conclui.
6. Registrar números medidos em `docs/STATE.md` (com a ressalva VM sem 3D) e comitar localmente sem push.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Mudar `target = content + extras`, a política de move, ou fazer o conteúdo esticar.
- Editar `protocols/*.xml`, ADR 0006, `Cargo.lock`, `rust-toolchain.toml`, `AGENTS.md`, security-model,
  packaging, `frame_resize_sync.py`.
- Dependências novas (crate/npm/apt), fork do Chromium, CEF OSR, WPE.
- Mexer no debounce de 200 ms **fora do caminho do grab**; reabrir 02c/02e/02f/02j por conta própria.
- Encerrar instância aberta pelo humano; matar processo por nome genérico; `sudo`/fora do repositório.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem resultado visível → pare e descreva o bloqueio (regra das duas tentativas: duas falhas
  da mesma abordagem => parar e pedir decisão).
- Dependências novas: nenhuma. Arquivos protegidos: ver AGENTS.md §6.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas
laterais (já anotadas). Ao final: atualizar STATE.md (§3, §4, §5, §6, §7, §9) e comitar localmente, sem push.
