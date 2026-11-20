# Tarefa 02j — diagnósticos nomeados do resize do frame

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.
> **RASCUNHO — aguarda o "ok" do humano. NÃO executar antes disso.**

## Objetivo (uma frase, verificável)
Todo configure da decoração que (a) não recebe `ack` em 100 ms, (b) não produz buffer no tamanho alvo em 300 ms
ou (c) mantém a textura do frame escalada no render passa a gerar **uma linha nomeada no log** do compositor
compilado com `--features bench`, com `window_id`, serial, tamanhos e tempo — sem mudar a política de resize/move.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-compositor/src/protocols/aero_frame.rs:59-118` — `send_frame_configure` emite `FRAME_CONFIGURE`
  e empilha `pending_configures`, hoje só `(serial, Instant)`: não há `window_id` nem tamanho para diagnosticar.
- `crates/aeroshell-compositor/src/protocols/aero_frame.rs:485-504` — `AckConfigure` emite `FRAME_ACK`.
- `crates/aeroshell-compositor/src/state.rs:368-386` — `FRAME_TIMEOUT` (100 ms) existe, mas percorre só
  `frame_probes`, que são configurates **sintéticos do bench**; os configures reais (`grab`/`commit`/`final`)
  não são cobertos. `FrameProbe` em `state.rs:60-72`. **Não apagar o diagnóstico antigo.**
- `crates/aeroshell-compositor/src/protocols/compositor.rs:40-80` — `FRAME_BUFFER_COMMIT` já calcula `expected`
  (conteúdo configurado + insets/shadow) e `ok`; `:81-124` emite `FRAME_COMMIT`. O formato atual
  (`FRAME_BUFFER_COMMIT buffer=... expected=... ok=... t=<ns>`) deve ser preservado (3 harnesses antigos dependem
  do prefixo).
- `crates/aeroshell-compositor/src/backend/winit.rs:225-259` — `RESIZE_GLUE` mede `target`, `buffer` e `rescaled`;
  é aqui que a escala 02e começa/termina e onde se mede a duração do episódio.
- `tests/integration/frame_configure_hygiene.py:38-40` — regra do humano: a decoração aparece 1–2 s depois que a
  janela abre. Todo harness novo espera **2,0 s** após o primeiro buffer antes de arrastar.
- `/tmp/opencode/aero-02f-visual.log` — arrasto real que o juiz negou: descompasso `|buffer-target|` p95 434 px,
  fator de escala p95 1,399; não há linha que diga *qual* modo de falha aconteceu.
- Prior art (contexto, **não** receita de implementação): `github.com/cprussin/domicile`,
  `docs/architecture/WINDOW-COMPOSITING.md`, `docs/architecture/ARCHITECTURE.md` — decoração e conteúdo na mesma
  cena, geometria de apresentação no compositor final, último buffer transformado até o novo chegar, e
  diagnósticos nomeados para "nunca fez ack" / "nunca desenhou no tamanho novo". Correção do humano: cprussin é
  contribuidor do Smithay; o criador é Victor Berger (elinorbgr). **Não copiar fork do Chromium nem CEF OSR.**
- ADRs: 0001 (Plano B), 0004 (frame = `layer_surface`), 0006 (composição elástica — política atual: o conteúdo
  NÃO estica; só a textura do FRAME estica transitoriamente e faz *snap* crisp depois). ADR 0006 é aditivo/proposto
  e **não será editado** nesta tarefa.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste primeiro, antes de qualquer edição no compositor.** Escrever os testes puros de `frame_diagnostics` e o
   harness `tests/integration/frame_diagnostics.py`, depois executar e salvar a saída crua:
   - `cargo test -p aeroshell-compositor frame_diagnostics` → FALHA (módulo/API ainda inexistente), em
     `/tmp/opencode/02j_diag_baseline_unit.log`.
   - `python3 tests/integration/frame_diagnostics.py` → `FALHA: FRAME_NEVER_ACKED ausente; FRAME_NEVER_DREW_AT_SIZE
     ausente; FRAME_STRETCH_ACTIVE ausente`, em `/tmp/opencode/02j_diag_baseline_harness.log`.
   Mostrar as duas saídas antes de instrumentar. Nenhuma amostra é descartada.
2. `cargo test -p aeroshell-compositor frame_diagnostics` → 4 testes OK, cada um com saída nomeada:
   - configure **sem ack em 100 ms** → exatamente 1 `FRAME_NEVER_ACKED` com `serial`, `window_id`, `w`, `h`,
     `waited_ms`;
   - **ack em 50 ms** → nenhum evento;
   - **sem buffer no tamanho alvo em 300 ms** → exatamente 1 `FRAME_NEVER_DREW_AT_SIZE` com `serial`,
     `window_id`, `target_w`, `target_h`, `buffer_w`, `buffer_h`, `waited_ms`;
   - **episódio de escala** (transição `rescaled false→true→false`) → exatamente 1 `FRAME_STRETCH_ACTIVE` com
     `window_id`, `start_t`, `end_t`, `duration_ms`, `max_scale`, `w`, `h`; dois episódios separados geram 2 linhas.
3. `python3 tests/integration/frame_diagnostics.py` → `VEREDITO: OK` em **3 execuções seguidas**, nenhuma amostra
   descartada, com `/tmp/opencode/02j_diag_final{1,2,3}.log`. O harness deve: (a) iniciar o compositor com
   `--features bench` e esperar 2,0 s após o primeiro `FRAME_BUFFER_COMMIT buffer=Some`; (b) fazer 14 passos de
   10 px a 50 ms na borda direita; (c) numa 2ª execução controlada, usar **injeção de falha só sob `bench`**
   (`AERO_DIAG_SUPPRESS_ACK_ONCE=1` e `AERO_DIAG_SUPPRESS_DRAW_ONCE=1`, ou nomes equivalentes documentados no
   harness) para tornar determinísticos o configure sem ack e o configure sem buffer no alvo; (d) imprimir as
   linhas cruas usadas no veredito e verificar campos obrigatórios, **nenhum serial duplicado** e pelo menos 1
   `FRAME_STRETCH_ACTIVE` com `duration_ms > 0` durante o arrasto. VM sem 3D: nenhuma conclusão de desempenho.
4. Vizinhos, com o formato de log antigo intacto: `python3 tests/integration/frame_resize_glue.py` → 3× OK;
   `python3 tests/integration/frame_configure_hygiene.py` → 3× OK; `frame_resize_edges.py`,
   `frame_follows_resize.py`, `frame_buttons.py`, `frame_window_id.py` → OK;
   `frame_resize_sync.py` → permanece `VEREDITO: FALHA` (métrica antiga, não enfraquecida).
5. `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0) e
   `cargo clippy --workspace --all-targets --features bench -- -D warnings` → EXIT=0, cada um numa única execução.
6. Nenhuma string nova em binário de produção: `cargo build -p aeroshell-compositor` → EXIT=0 e
   `strings target/debug/aeroshell-compositor | grep -cE 'FRAME_NEVER_ACKED|FRAME_NEVER_DREW_AT_SIZE|FRAME_STRETCH_ACTIVE'`
   → `0`. Todas as novas `eprintln!` ficam sob `cfg(feature = "bench")`.
7. **Não há critério visual novo**: a tarefa só instrumenta. Nenhuma mudança de aparência, geometria, cadência ou
   textura; o compositor de validação da 02f (PID 72480) continua aberto e não é encerrado pelo agente.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Fork do Chromium, CEF OSR, trocar WebKitGTK, qualquer dependência nova (crate/npm/apt).
- 9-slice/`CropRenderElement`, 02c, 02d, 02g, 02h, 02i, 05. Não reabrir 02e/02f nem mexer no debounce de 200 ms.
- Mudar a política de escala: conteúdo nunca estica; move intacto (geometria no Rust, um pass); ADR 0006 preservado.
- Editar `protocols/*.xml`, `Cargo.lock`, `rust-toolchain.toml`, ADR aceito, `docs/security-model.md`, `packaging/*`,
  `AGENTS.md` ou `tests/integration/frame_resize_sync.py`.
- Apagar/renomear `FRAME_TIMEOUT`, `FRAME_ACK`, `FRAME_BUFFER_COMMIT`, `FRAME_COMMIT` ou `RESIZE_GLUE`, ou
  enfraquecer asserções de teste existentes.
- Encerrar PID 72480, matar processos por nome genérico, mudar `WAYLAND_DISPLAY` global ou usar `sudo`.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem diagnóstico auditável → pare e descreva o bloqueio (regra das duas tentativas: duas falhas da
  mesma abordagem => parar e pedir decisão).
- Dependências novas: nenhuma.
- Arquivos protegidos: ver AGENTS.md §6.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos e o que ver no log) · o que quebrou/não foi testado ·
descobertas laterais (já anotadas em STATE.md §6). Ao final: atualizar STATE.md (§3, §4, §5, §6, §7, §9) e
comitar localmente, sem push. Parar; não iniciar a próxima fase.
