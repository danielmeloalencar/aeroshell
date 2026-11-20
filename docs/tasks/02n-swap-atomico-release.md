# Tarefa 02n — swap atômico no release (completa buffer-congelado/9-slice)

> 4ª correção, autorizada explicitamente pelo humano em 2026-11-20. NÃO é uma nova família: completa a 02m
> (press/arrasto já aprovados no juiz) atacando a mesma causa-raiz — apresentar buffer mid-resize.

## Objetivo (uma frase, verificável)
Ao soltar um resize após arrastar, nenhum frame é apresentado com o buffer inconsistente em relação à
referência/versão das slices (0 frames inconsistentes, baseline medido ≥1), com swap atômico num único render
e `source=final` adiado para idle, provado por medição objetiva e pelo juiz visual, sem mudar press/arrasto,
9-slice, target, move, conteúdo, protocolo, shell, deps nem ADR 0006.

## Contexto mínimo
- `crates/aeroshell-compositor/src/grab.rs:403-420` — `unset` do resize: hoje zera `resize_grab_active` e envia
  `source=final` imediatamente.
- `crates/aeroshell-compositor/src/backend/winit.rs:238-452` — render: `rescaled = buffer != target` (:248-251);
  se `rescaled` → 9-slice (:359-449), senão `Surface(el)` nativo (:450-452). É aqui que o swap acontece.
- `crates/aeroshell-compositor/src/state.rs:31-61,128-131` — `FrameGeometry` (sem versão) e `resize_grab_active`
  (lido só no commit-hook `compositor.rs:215`).
- `crates/aeroshell-compositor/src/protocols/aero_frame.rs:80-127,434-444,505-528` — `send_frame_configure`,
  `set_titlebar_slices` (uma vez no associate) e `AckConfigure` (serial).
- Evidência: `/tmp/gh-comp-110069.log` — após `source=final w=756 h=546`, buffer 708x530 com
  `rescaled=true sx=1.0862` por 10 frames ≈79,7 ms, depois salta para 768x582 `rescaled=false`
  (layout/slices inconsistentes por 1+ frame). Juiz (2026-11-20): "arrastar e soltar sem flicker" pendente;
  press/arrasto já aprovado ("resolveu totalmente").

## Critério de aceite (comandos exatos + saída esperada)
1. Teste PRIMEIRO visto falhando: `python3 tests/integration/frame_release_swap_hygiene.py` → baseline FALHA
   com `frames_buffer_inconsistente >= 1` (log cru do swap em `/tmp/gh-comp-110069.log`). Depois da correção:
   3 execuções seguidas → `frames_buffer_inconsistente=0 swaps_atomicos=1 VEREDITO: OK`.
2. Testes vizinhos: `frame_grab_configure_hygiene.py`, `frame_configure_hygiene.py`, `frame_buttons.py`,
   `frame_window_id.py`, `frame_resize_glue.py`, `frame_initial_buffer.py` OK; `frame_resize_sync.py` FALHA
   esperada (métrica antiga); `frame_follows_resize.py` flake conhecido de captura `detected=None`.
3. `bash scripts/check.sh` verde (bun 9 pass/0 fail).
4. `cargo clippy --workspace --all-targets --features bench -- -D warnings` → EXIT=0.
5. Teste manual (juiz visual, obrigatório): abrir instância do RUNBOOK com 2 `foot`, deixar aberta; humano
   clica, segura, arrasta e SOLTA bordas da 1ª e 2ª janela → sem flicker no soltar, sem transiente no press;
   dar "ok" explícito. Quem fecha a aplicação é o humano (AGENTS.md §0.9).

## Correção (compositor; sem protocolo/shell/deps/ADR)
- No `unset`, NÃO zerar `resize_grab_active` de imediato: entrar em estado `release_pending` que mantém o
  buffer congelado apresentado via 9-slice (já correto no target final). NÃO trocar para nativo só por
  `buffer == target`.
- Só aposentar o congelado quando `buffer_size == total_final` E a referência/versão das slices casar com
  esse buffer (estado interno do compositor — sem mudar protocolo). Swap atômico num único render.
- Adiar o configure `source=final` para idle (~150-250 ms após o release); manter o request, só fora do
  momento visual. `commit-hook` segue calado durante o grab.
- Timeout de segurança (~400 ms): aposentar o congelado de qualquer jeito, com log quando disparar.
- Estado novo, tratar explicitamente: novo grab durante `release_pending` (cancelar final pendente + timeout,
  seguir fluxo normal); janela destruída durante `release_pending` (cancelar timers sem pânico); coerência
  idle/timeout (timeout > idle + tempo esperado do frame ~80 ms).
- NÃO mexer: press/arrasto (02m), 9-slice (02k/02l), `target = content + extras`, move, conteúdo, protocolo,
  ADR 0006.

## Fora de escopo (NÃO faça)
- Mudar a decisão do press (02m aprovada), o 9-slice da 02k/02l, `target`, move, conteúdo HTML, protocolo
  (`protocols/*.xml`), shell, dependências ou ADRs.
- Abrir 5ª correção sem nova decisão do humano se o juiz ainda vir flicker.

## Ressalvas a registrar no veredito
- "Visualmente idêntico em tema uniforme" é HIPÓTESE: com gradiente, sombra ou blur o 9-slice de 708 e o
  buffer nativo de 768 podem diferir levemente. Confirmar no juiz.
- Sem medição em GPU real (VM sem 3D).

## Limites
- Tempo: ~2 h sem resultado visível → pare e descreva o bloqueio.
- Regra das duas tentativas ativa: se a mesma abordagem falhar 2×, pare e peça decisão.
- Dependências novas: nenhuma. Arquivos protegidos: ver AGENTS.md §6.

## Entrega
Relatório curto: o que funciona · como testar à mão · o que quebrou/não foi testado · descobertas laterais.
Ao final: atualizar STATE.md (§3, §4, §5, §6, §7, §9) e comitar localmente (sem push).
