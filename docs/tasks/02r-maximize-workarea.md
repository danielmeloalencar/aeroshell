# Tarefa 02r (RASCUNHO — aguardando "ok" para executar) — maximizar usa a área útil do output

> Cartão novo, aberto a pedido do humano em 2026-11-22 após a 02q. Descoberta registrada em
> `STATE.md` §6: maximizar **só aumenta o tamanho**; a janela não volta para a origem e vaza pelo canto.
> Não é regressão da 02q — `window_ipc.rs` não foi tocado lá.

## Objetivo (uma frase, verificável)
Ao maximizar, a janela (cliente + decoração) ocupa exatamente a área não exclusiva do output — começando
na origem dessa área e sem vazar para baixo do painel — e ao desmaximizar volta à posição e ao tamanho
que tinha antes.

## Contexto mínimo
- `crates/aeroshell-compositor/src/window_ipc.rs:128-151` — hoje só faz `state.states.set(Maximized)` +
  `state.size = Some(output_size)` + `send_configure()`. Não mexe na posição nem desconta o painel.
  Também lê o toggle de `with_pending_state`, não de `current_state()` (`smithay-0.7.0/.../xdg/mod.rs:1736`).
- `crates/aeroshell-compositor/src/protocols/xdg_shell.rs:53-76` — a janela nasce na cascata
  `(60+40n, 80+40n)` via `space.map_element(window, loc, false)`.
- Fonte da API (Smithay 0.7.0 fixado): `Space::map_element` **já serve para reposicionar** um elemento
  mapeado (`desktop/space/mod.rs:96-119`); `element_location` em `:227`; `layer_map_for_output` em
  `desktop/wayland/layer.rs:48`; `LayerMap::non_exclusive_zone()` em `:135-138` (é um `Rectangle` lógico).
- Frame é layer surface, mas **não entra no LayerMap** (exceção documentada no `aero_frame_v1.xml`):
  `non_exclusive_zone()` vê só o painel. As insets do frame vêm de `frame_geometries`/`default_frame_insets`.
- Juiz: humano relatou no tty3 *"os botoes respondem, mas a janela não é reposicionada ao maximizar"*.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste primeiro:** criar `tests/integration/maximize_workarea.py` e rodar no código atual →
   `VEREDITO: FALHA`, mostrando cru: posição da janela maximizada `(60,80)` contra origem esperada da
   área útil, e `target` do frame maior que essa área (vazamento). Preservar a amostra em `/tmp/opencode/02r/`.
2. Depois da correção, o MESMO comando 3× seguidas, sem descartar amostras → `VEREDITO: OK`:
   - `frame_target == non_exclusive_zone` (posição e tamanho), com tolerância de arredondamento do foot;
   - cliente configurado com `zone.size − insets do frame`, nunca com o tamanho bruto do output;
   - ao desmaximizar, posição e tamanho voltam aos valores medidos antes do maximize;
   - nenhuma janela fora do output; nenhuma exceção de protocolo no log.
3. Juiz visual obrigatório: nested pelo RUNBOOK (com `AERO_WINDOW_IPC_SOCK`) e tty3 via `~/u`;
   aplicação deixada aberta. Humano confirma: barra no topo da área útil, nada embaixo do painel,
   desmaximizar devolve a janela ao lugar. Quem fecha é o humano.
4. Vizinhos: `frame_buttons.py`, `frame_fixed_canvas_hygiene.py`, `frame_resize_glue.py`,
   `frame_window_id.py`, `frame_cold_start_stages.py` (este último segue FALHA só no latch quente,
   conforme 02q) → sem regressões novas.
5. `bash scripts/check.sh` → `OK: scripts/check.sh passou` numa execução;
   `cargo clippy --workspace --all-targets --features bench -- -D warnings` → 0;
   `cargo check --workspace --no-default-features` → 0.

## Fora de escopo
- Fullscreen, tiling, arrastar janela maximizada, multi-output (usar o primeiro output, como hoje).
- Mudar insets/9-slice/canvas/ADR 0007, protocolo, dependências, segurança, painel React.
- Perseguir números de desempenho em VM sem 3D.

## Se você encontrar OUTRO problema
Anotar em `STATE.md` §6 (1–3 linhas: sintoma, onde, hipótese) e VOLTAR. Só investigar se bloquear o
aceite, e por no máximo 15 min. Regra das duas tentativas: após 2 falhas da mesma abordagem, parar.

## Limites
- Tempo: 2 h sem resultado visível → parar e descrever o bloqueio.
- Uma correção por vez, teste primeiro. Sem `sleep`, sem constante de tema duplicada, sem `allow`/`unwrap`
  novos. Sem mexer em arquivos protegidos. Se exigir ADR, **parar e perguntar**.

## Entrega
Resultado · teste manual exato · confirmação visual · provas cruas antes/depois · não executado/não medido ·
descobertas laterais (já em §6) · próximo passo sem executar. Atualizar `STATE.md` §§3–7 e §9;
commit local sem push.
