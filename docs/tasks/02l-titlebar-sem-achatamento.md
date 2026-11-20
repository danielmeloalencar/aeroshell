# Tarefa 02l — titlebar sem achatamento horizontal durante o resize

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.
> RASCUNHO — aguarda confirmação do humano para virar cartão executável (AGENTS.md §0.2).

## Objetivo (uma frase, verificável)
Durante o arrasto de resize, o texto e os botões da titlebar mantêm a largura que tinham no buffer congelado
(nenhum achatamento/estiramento horizontal), enquanto as bordas continuam com espessura constante — provado por
medição objetiva nova e pelo juiz visual, sem mudar `target = content + extras`.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-compositor/src/backend/winit.rs` — 02k: 9-slice com 9 fatias; a fatia `titlebar` usa
  `(slice_sx, 1.0)`, ou seja, estica em X todo o trecho entre os insets laterais. É isso que achata o HTML.
- `crates/aeroshell-shell/examples/frame_webview.rs:44-48,191-195,306` — `TITLEBAR_H=30`, `BORDER=6`,
  `RIGHT_CONTROLS=96`; a zona de texto fica em `x < tw - RIGHT_CONTROLS` e a dos botões à direita. O shell
  conhece esses limites; o compositor, não.
- `crates/aeroshell-compositor/src/state.rs:33-57` — `FrameGeometry` hoje só tem `insets` e `shadow`.
- `protocols/aero_frame_v1.xml:117-137` — `set_insets`/`set_shadow_extents` são o padrão para declarar
  geometria pelo shell. **Arquivo protegido (AGENTS.md §6): editar só com aprovação explícita.**
- `docs/STATE.md` §6/§7 — relato cru do juiz e lacuna da medição auto-referencial da 02k.
- ADR 0006 é proposto/aditivo: NÃO editar. Se a solução mudar a decisão, escrever ADR novo.

## Decisão 0 (BLOQUEANTE — responder antes de qualquer edição)
Como o shell informa ao compositor as linhas de corte extras da titlebar? Padrão que eu proporia:
novo request `set_titlebar_slices(left_fixed, right_fixed)` em `protocols/aero_frame_v1.xml`, estado pendente
aplicado no `wl_surface.commit`, com campos correspondentes em `FrameGeometry`; o `frame_webview` envia as
larguras das zonas protegidas (texto e botões) e o compositor estica SOMENTE o intervalo entre elas.
Alternativa sem tocar no protocolo (inferir da `input_region` do `wl_surface`) acopla mecanismo ao DOM e
**não é recomendada** sem aprovação. Se a decisão 0 não for dada, PARE e pergunte.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste primeiro, visto falhando:** estender `tests/integration/frame_resize_glue.py` (ou criar
   `frame_titlebar_fidelity.py`) para medir, por frame do arrasto, a largura DESTINO das zonas protegidas de
   texto e botões da titlebar. Rodar no código atual da 02k e salvar baseline **FALHA** (zona central
   esticada: `title_text_w` cresce com `slice_sx`). Nenhuma amostra descartada; 3 execuções seguidas no fim.
2. Com a decisão 0 aprovada, declarar as linhas de corte e alterar as specs do 9-slice: zonas de texto e
   botões com escala `(1.0, 1.0)`; somente o intervalo entre elas com `(slice_sx, 1.0)`. Bordas laterais,
   base, cantos e centro continuam como na 02k. `target = content + extras` inalterado.
3. `python3 tests/integration/frame_resize_glue.py` 3× → `VEREDITO: OK`, com `edge_l/edge_r=6,00`,
   `title_h=30,00`, `SLICE_COMPOSE n=9`, `title_text_w` e `title_buttons_w` constantes no grab e o delta
   de largura absorvido só pela faixa central. Vizinhos: `frame_configure_hygiene.py`,
   `frame_buttons.py`, `frame_follows_resize.py`, `frame_window_id.py` OK; `frame_resize_sync.py` segue FALHA.
4. `bash scripts/check.sh` → `OK: scripts/check.sh passou`;
   `cargo clippy --workspace --all-targets --features bench -- -D warnings` → EXIT=0.
5. **JUIZ VISUAL (humano):** executar o nested do RUNBOOK (comando abaixo) com 2 `foot`, deixar a aplicação
   **aberta**, arrastar lenta e rapidamente as bordas e a barra. O humano deve ver o texto e os botões da
   barra com a mesma largura do início do arrasto, sem achatar nem esticar, e as bordas com espessura
   constante. Só com o "ok" explícito o item fecha; caso contrário fica em STATE.md §7 (AGENTS.md §0.9).
6. Registrar números medidos em `docs/STATE.md` (com a ressalva VM sem 3D) e comitar localmente, sem push.

Comando manual do RUNBOOK (deixar aberto; quem fecha é o humano):
```
cargo build --workspace --examples
LD_PRELOAD=libgtk4-layer-shell.so.0 target/debug/aeroshell-compositor \
  --backend winit --socket wayland-aero-02l \
  --shell target/debug/examples/frame_webview \
  --shell-env LD_PRELOAD=libgtk4-layer-shell.so.0 \
  --command foot
```

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Mudar `target = content + extras`, a política de move, ou fazer o conteúdo esticar.
- Reabrir 02c/02e/02f/02j/02k por conta própria; mudar o debounce fora do caminho do grab.
- Editar ADR 0006, `security-model.md`, `packaging/*`, `rust-toolchain.toml`, `Cargo.lock`, `AGENTS.md`.
- Dependências novas (crate/npm/apt), fork do Chromium, CEF OSR, WPE.
- Hardcodar `RIGHT_CONTROLS=96` no compositor como substituto da decisão 0.
- Encerrar instância aberta pelo humano; matar processo por nome genérico; `sudo`/fora do repositório.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem resultado visível → pare e descreva o bloqueio. Regra das duas tentativas: se a mesma
  abordagem falhar 2×, PARE, escreva o que tentou e peça decisão (AGENTS.md §3.12 / §4.1).
- Decisão 0 é pré-requisito: sem ela, NÃO editar `protocols/*.xml` nem ADR.
- Dependências novas: nenhuma. Arquivos protegidos: ver AGENTS.md §6.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais
(já anotadas). Ao final: atualizar STATE.md (§3, §4, §5, §6, §7, §9) e comitar localmente (sem push).
