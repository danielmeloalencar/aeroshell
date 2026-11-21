# F0 — relatório go/no-go (fechamento formal em 2026-11-21)

> Consolida números **já medidos** e decisões **já tomadas**. Não reexecuta spikes, não reabre cartões.
> Estado atual: `docs/STATE.md`. Histórico bruto: `docs/progress.md` (arquivo histórico, não fonte de decisão).
> Ambiente: Ubuntu 26.04.1 em VirtualBox **sem 3D**. Todo número de tempo/RAM abaixo é relativo a esta VM;
> **nenhuma conclusão de desempenho em GPU real** foi tirada daqui.

## Veredito

**F0 FECHADA — GO para F1.** Decisão do humano em 2026-11-21: *"Com isso fechamos a fase 0 f0 do plano
master e ja poderemos ir para o f1"* (`docs/STATE.md` §3, `docs/plano-mestre.md` §7, commit `aabbe26`).
Este relatório completa a forma que faltava: quadro S1–S5 com números, ratificação dos ADRs e
deferimentos explícitos de S4/S5. A F1 continua exigindo cartão próprio e aprovação por tarefa.

## 1. Quadro S1–S5 (dados já medidos)

| Spike | Evidência / números medidos | Veredito |
|---|---|---|
| **S1 — mecanismo** | Compositor nested sobe com socket próprio `wayland-aero-0`, globals `wl_compositor`/`wl_shm`/`xdg_wm_base`/`zwlr_layer_shell_v1`/`wl_output`/seat. Cliente `zenity` conectou; EGL/GLES inicializou (VMware SVGA3D/LLVM — VM sem 3D) e o diálogo com cantos arredondados apareceu dentro da janela, com transparência fora deles. `cargo check --workspace --all-targets` exit 0. `docs/progress.md:26-46` | **GO** |
| **S2 — WebView** | **NO-GO do caminho WPE original** (ausente no Ubuntu 26.04; decisão congelada em `AGENTS.md` §2). **GO da rota substituta**: WebKitGTK 6.0 como cliente Wayland + canvas fixo (ADR 0007). PSS em VM, 3 repetições: N=1 sem related `275937/275967/276173 kB`; N=20 sem related `1009834/1011756/1010505 kB` (20 WebProcesses); N=20 **com related-view** `508554/511498/504604 kB` (1 WebProcess); atlas 1 WebView/20 frames `308178/308071/308259 kB`, inserir 20 componentes 0,2–0,4 ms, 1 componente 0–0,01 ms, superfície 1000×613 ARGB8888 = 2.452.000 bytes. Release, 1 painel: PSS soma `236891 kB` (`docs/adr/0001-go-no-go.md:24-41`, `docs/progress.md:48-82,104-111`). | **GO (rota substituta)** |
| **S3 — input/alfa** | Regiões de entrada derivadas do DOM (`data-aero-interactive`), `MutationObserver`/`ResizeObserver`, cursor mão no botão, contador de clique, área transparente não captura clique — confirmação **manual** do humano. A medição alvo p99 < 1 ms/evento **não foi feita** com número; o gate foi funcional (`STATE.md` §7, `docs/progress.md:174-197`). | **GO funcional; p99 não medido** |
| **S4 — IPC Rust↔Bun** | O que existe é request/response **síncrico** (`u32` + JSON) com types gerados por `ts-rs`: `--list/--focus/--close` provados em `tests/integration/sdk_cli.py`; `bun test` 9 pass. O spike completo do plano (MessagePack/rmp-serde, 100k round-trips, p99 < 1 ms, 0 perdas, reconexão) **não foi executado**; latência/perda/reconexão **não medidas** (`STATE.md` §3/§6, §7). | **DEFERIDO → F2/F9** |
| **S5 (numeração antiga) — frames/PSS** | Mesmos números de PSS do S2 acima. O spike do frame foi fechado pelo juiz: resize com canvas fixo 9-slice (02p: 3 execuções, 8/8 motions, `changed_pixels=0`, juiz *"teste visual ok. Passou"*) e cold-start (02q: placeholder + âncora oculta + `related-view` + pool; juiz *"visualmente perfeito e bem otimizado"* / *"ok. aprovado"*). O cronômetro map→1º render (>300 ms) segue divergente e registrado como lacuna — **não é critério fechado por medição** (`STATE.md` §3/§7). | **GO no juiz; cronômetro em lacuna** |
| **S5 (`plano-mestre.md` §7.2) — PAM/greetd** | Handshake greetd, conversa PAM em thread dedicada e zero vazamento de segredo **não foram executados**. ADR 0004 registra a decisão de arquitetura do protocolo privado; ADR 0005 registra o supervisor (escopo só do shell). Lock/greeter/PAM são F5/F6. | **DEFERIDO → F5/F6** |

**Nota de numeração:** "S5" aparece com dois sentidos nos documentos — o experimento frames/PSS
(`docs/adr/0001-go-no-go.md`, `docs/progress.md`) e o spike PAM/greetd (`docs/plano-mestre.md` §7.2).
Ambos estão listados acima com rótulos distintos; **não foram unificados** para não reescrever histórico.

## 2. Ratificação dos ADRs

Nesta sessão foi autorizada a edição **somente da linha de status** de dois ADRs; o conteúdo técnico
de todos eles permanece intacto. O diff foi mostrado no relatório da sessão antes do commit.

| ADR | Status anterior | Status registrado |
|---|---|---|
| 0001 — Go/No-Go do S5 e arquitetura de frames | aceito provisoriamente (2026-11-18) | **ratificado pelo relatório F0 (2026-11-21)** |
| 0002 — pins e licenças | aceito (F0) | inalterado |
| 0003 — blur/protocolo | aceito (F0/S5) | inalterado |
| 0004 — associate sem role | aceito (2026-11-18) | inalterado |
| 0005 — supervisor interno | aceito (2026-11-18) | inalterado |
| 0006 — composição elástica do frame | proposto (2026-11-20) | **superado por 0007 (2026-11-20)** |
| 0007 — frame com canvas fixo | aceito explicitamente (02p, 2026-11-20) | inalterado |

## 3. Deferimentos formais

- **S4 → F2 (ponte web + SDK v0) e F9 (endurecimento).** Justificativa: o IPC síncrico atual atende à
  fatia vertical; streaming de eventos, codec MessagePack, 100k round-trips e p99 < 1 ms são requisitos
  da ponte web e do endurecimento, **não do núcleo F1**. Nenhuma medição da métrica do spike foi feita;
  isso fica registrado em `STATE.md` §7, não como "passou".
- **S5/PAM/greetd → F5 (IDLE+LOCK) e F6 (GREETER).** Justificativa: lock e greeter são processos próprios
  com máquina de estados no compositor; tocar em PAM/`greetd` exige ambiente isolado e testes de segurança,
  fora do núcleo de render/protocolos da F1. Nenhum passo fora do repositório foi executado.

## 4. Lacunas preservadas (não são conclusões)

- VM sem 3D: funcionamento validado; **FPS, GPU ociosa e PSS em hardware real não medidos**.
- `frame_cold_start.py` falha o sub-cronômetro >300 ms (374–415 ms na via A) e `frame_resize_sync.py`
  falha pela métrica antiga de p95. São **baseline-falha, não gates**: 02p/02q foram fechadas pelo
  juiz visual, não por esses harnesses. Nenhum número foi editado ou descartado.
- Intermitência do primeiro buffer do frame (2 timeouts sem causa identificada) segue registrada em
  `STATE.md` §6; não foi declarada corrigida.
- `winit 0.30` não expõe `Window::set_size`; multi-output real entra na F1.
