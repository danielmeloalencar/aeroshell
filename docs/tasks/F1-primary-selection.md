# Tarefa F1-primary-selection — subcartão de `F1-gap-protocolos.md` (RASCUNHO — aguarda "ok")

> Um agente deve executar lendo só AGENTS.md, STATE.md, RUNBOOK.md,
> `docs/tasks/F1-gap-protocolos.md` e este arquivo. Uma tarefa por sessão; se o objetivo mudar, PARE e pergunte.
> Próximo da fila após `F1-cursor-shape` (FECHADA pelo juiz em 2026-11-22).

## Objetivo (uma frase, verificável)
O compositor anuncia `wp-primary-selection-v1` (zwp_primary_selection_device_manager_v1), o `foot` para de
imprimir "compositor does not implement the primary selection interface" e a seleção primária acompanha o
foco de teclado, sem regredir o clipboard/DnD (`wl_data_device`) já existente.

## Contexto mínimo — premissas CONFIRMADAS na fonte fixada (Smithay 0.7.0)
- `wayland/selection/primary_selection/mod.rs:92-95` — `PrimarySelectionHandler: SeatHandler + SelectionHandler`
  com um único método obrigatório: `fn primary_selection_state(&self) -> &PrimarySelectionState`.
- `primary_selection/mod.rs:98-119` — `PrimarySelectionState::new::<D>(&dh)` registra o global (versão 1) e
  exige `D: PrimarySelectionHandler + GlobalDispatch<…, PrimaryDeviceManagerGlobalData>`.
- `primary_selection/mod.rs:395-410` — `delegate_primary_selection!(AeroState)` gera 4 delegates
  (manager global, manager, device, source).
- `primary_selection/mod.rs:154-168` — `set_primary_focus::<D>(&dh, seat, Option<Client>)` é a função
  solta que atualiza o foco; o doc em `:9-11` manda chamá-la no hook de foco do teclado.
- `primary_selection/device.rs:56-77` — o Smithay só aceita `set_selection` de cliente com foco de teclado;
  por isso o foco de primária DEVE seguir o foco de teclado (não basta anunciar o global).
- Código do repo: `state.rs:171-181` (globals/estado), `protocols/seat.rs:37-49` (`focus_changed` já chama
  `set_data_device_focus`; acrescentar `set_primary_focus`), `protocols/seat.rs:53-66`
  (`SelectionHandler` + `DataDeviceHandler` já existem).

## Como usar `/home/daniel/Projetos/domicile-engine` — guia, NÃO receita
Determinação do humano (2026-11-22): o domicile serve para **invariantes e lições arquiteturais**, nunca
como código a copiar. Aplicação prática neste cartão:
- **Transferível:** a ideia de que a seleção (clipboard/primária) pertence ao cliente com foco de teclado e
  que o compositor é dono da geometria/apresentação eventual; e o hábito de **diagnósticos nomeados**
  (`PRIMARY_FOCUS`) para responder "por que este cliente não recebeu a seleção". Já registrado em
  `STATE.md §6`: "geometria de apresentação dona do compositor final + diagnósticos nomeados".
- **NÃO transferível:** fork do Chromium/Ozone, `DrmCursorFactory`, CEF OSR, nomes/esquemas do fork,
  `cc::SurfaceLayer`/`CompositorFrame`, prior art do fork. Copiá-los não eliminaria o invariante da Tarefa 1
  e contradiria a premissa de WebView nativa (WebKitGTK 6.0 como cliente Wayland) — `STATE.md §6`.
- **Fonte executável deste cartão:** exclusivamente o Smithay **0.7.0 fixado** (citado linha a linha acima) e
  o código do AeroShell. Onde o domicile e o Smithay divergirem, prevalece o Smithay fixado + decisões
  congeladas do `AGENTS.md §2`.

## Escopo (mínimo)
1. Campo `pub primary_selection_state: PrimarySelectionState` em `AeroState` + `PrimarySelectionState::new::<Self>(&dh)`.
2. `impl PrimarySelectionHandler for AeroState { fn primary_selection_state(…) }` + `delegate_primary_selection!(AeroState)` em `seat.rs`.
3. Em `focus_changed`, chamar `set_primary_focus(&self.display_handle, seat, client)` ao lado do `set_data_device_focus` existente.
4. Log sob `cfg(feature = "bench")`: `PRIMARY_FOCUS t=… client=<id|none>` — sem imprimir nada em produção.

## Critério de aceite (comandos exatos + saída esperada)
1. **Teste-first (pré-filtro no nested, executável nesta VM):** com o nested do RUNBOOK e `foot` no socket do
   AeroShell, capturar `stderr` do `foot` ANTES → `warn: wayland.c:1777: compositor does not implement the
   primary selection interface` (amostra crua já registrada: `/tmp/opencode/f1-cursor/foot-baseline.log:1`).
   Depois da implementação, o mesmo comando → essa linha **ausente**; as outras mensagens permanecem
   (fora de escopo). Colar as duas saídas cruas no relatório.
2. **Foco:** com `--features bench`, `PRIMARY_FOCUS …` aparece quando o `foot` recebe/perde foco; sem `bench`,
   `strings` do binário não contém esse prefixo.
3. **Não regressão:** `wl-copy "x" | wl-paste` (clipboard normal, via `wl_data_device`) continua funcionando
   com o nested; `bash scripts/check.sh` verde; clippy default/bench `-D warnings` EXIT=0;
   `cargo check --workspace --no-default-features` EXIT=0; os harnesses de frame do RUNBOOK seguem OK.
4. Round-trip de **primary** (`wl-copy --primary` → `wl-paste --primary`) é desejável; **se o Smithay recusar
   por falta de foco de teclado do `wl-copy`** (`device.rs:63`), registrar a limitação em `STATE.md §7` com a
   saída crua, em vez de contornar com atalho de privilégio (AGENTS.md §3.5).
5. Commit pequeno por etapa validada (regra 10), sem push.

## Juiz visual
Esta tarefa **não tem critério visual** — o critério é textual (aviso do `foot` + log bench). Não abrir
aplicação para "olhar tela" e não encerrar nada. Se o humano quiser, pode rodar `~/u` no tty3 e comparar o
stdout do `foot`; o veredito final do texto é do humano, não do agente.

## Fora de escopo
- Os outros avisos do `foot` (activation/urgência, fractional-scale, toplevel-icon, IME) — cartões separados.
- Mexer em `wl_data_device`, DnD, cursor, frame, 9-slice, lock/PAM ou no bug do conteúdo F1d3.
- Dependência nova, `protocols/*.xml`, ADR, `sudo`/VT/push.

## Se encontrar OUTRO problema
Anotar 1 a 3 linhas em `docs/STATE.md §6` (sintoma, onde, hipótese) e VOLTAR. No máximo 15 minutos de desvio.

## Limites
- Tempo: 2 h sem o aviso desaparecer → PARAR e reportar com dados; sem terceira via por conta própria.
- Dependências: nenhuma.

## Entrega
Relatório curto: o que funciona · como testar à mão · o que NÃO foi testado · gate colado · descobertas
laterais. Ao final: `STATE.md §3/§7/§9`, commit local, e PARAR (não abrir o próximo cartão sem ok).
