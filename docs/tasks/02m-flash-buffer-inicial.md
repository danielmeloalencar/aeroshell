# Tarefa 02m — flash do buffer inicial 200×200 na primeira janela decorada

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.
> RASCUNHO — aguarda confirmação do humano para virar cartão executável (AGENTS.md §0.2).
> Decisão do humano em 2026-11-20: abrir cartão novo, mantendo a 02l intacta.

## Objetivo (uma frase, verificável)
Ao clicar/arrastar as bordas da decoração não há transiente: ZERO configures da decoração entre `press` e
`release` (só o `source=final` no unset), provado por medição objetiva nova e pelo juiz visual, sem mudar
`target = content + extras`, protocolo, shell, ADR 0006 nem a 02l.

## DECISÃO DO HUMANO (2026-11-20) — substitui a decisão 0 original
O humano forneceu a especificação técnica abaixo depois que a correção no shell (buffer 200×200 eliminado,
3× OK) AINDA FALHOU no juiz visual ("Ainda pisca"):

> Sintoma (juiz): ao clicar na borda, a decoração inteira encolhe/achata a titlebar por milissegundos e depois
> corrige. Causa: o configure de "congelar para tamanho grande" no início do grab força resize GTK/WebKit; por
> alguns frames o buffer é grande com layout ANTIGO, e o 9-slice recorta pixels errados (cap direito vazio,
> miolo sobre conteúdo velho).
> Correção (compositor, sem protocolo/shell/deps): REMOVER o `send_frame_configure` de início de grab (o
> "freeze grande"). Durante o grab INTEIRO não enviar configure nenhum: o buffer fica congelado no último
> tamanho assentado. O 9-slice já funciona com buffer < target (caps em (1,1) nas bordas do target; só o miolo
> estica). Vale para crescer e encolher. MANTER só o configure `source=final` no unset/release (shell repinta e
> re-mede slices). commit-hook continua calado durante o grab (`resize_grab_active`). Não mexer em target, move,
> conteúdo, 9-slice da 02k/02l, ADR 0006.

Código lido que sustenta a especificação: `grab.rs:470` (`send_frame_configure(..., "grab-start")`),
`grab.rs:403-420` (`unset` com `source=final`), `winit.rs:244-289` (9-slice compõe no `target` com `buffer`
possivelmente menor, `delta_rx` para bordas presas à direita/inferior). NÃO inventar API.

## Contexto mínimo (arquivos que importam)
- `docs/STATE.md` §6/§7 — relato cru do juiz e reprodução com medição (7 frames `buf=200x200`,
  `slice_scale_x=3.7021`, `slice_scale_y=3.0122` em `window_id=1`, ~5,2 ms).
- `/tmp/opencode/aero-02l-visual.log` — evidência crua do teste visual do humano; transição
  `buf=200x200 rescaled=true` → `buf=708x530 rescaled=false` em t=1795160630897993417…1795160630949615576.
- `crates/aeroshell-shell/examples/frame_webview.rs:369-400` — `open_frame_window` cria a
  `gtk4::ApplicationWindow` sem tamanho explícito e a `WebView` com `WebView::builder()`; o primeiro buffer
  observado é 200×200.
- `crates/aeroshell-shell/examples/frame_webview.rs:243-262` — o `FrameEvent::Configure` só chama
  `entry.window.set_default_size(total_w, total_h)`; `set_default_size` define o tamanho padrão, e a janela
  pode já estar realizada quando ele chega (Hipótese a confirmar no spike — NÃO afirmar sem evidência).
- `crates/aeroshell-compositor/src/backend/winit.rs:238-272` — `target = geo.size + extras`, `buffer =
  el.buffer_size()` e `rescaled = buffer != target`; quando o buffer é 200×200, o 9-slice escala a moldura
  inteira. `SLICE_GEOM` confirma que as fatias da 02l estão corretas — o defeito é a origem do buffer.
- `docs/STATE.md` §6 — hipótese anterior (geometria alvo mudar no grab) foi **refutada** pelo diagnóstico
  com press sem motion (`/tmp/opencode/02l_flash_probe1.log`): `target` estável e `SLICE_GEOM` 126/480/102.
- ADR 0006 é proposto/aditivo: NÃO editar. `protocols/aero_frame_v1.xml` NÃO deve ser alterado nesta tarefa.

## Decisão 0 (BLOQUEANTE — responder antes de qualquer edição)
Onde evitar o buffer 200×200? Opções:
1. **Shell (recomendado, a confirmar por spike):** garantir que a WebView/janela já tenha o tamanho do
   configure antes do primeiro commit — seja criando-a com o tamanho certo, seja usando a API GTK4 adequada
   no momento certo (ex.: `set_size_request` antes do realize). A chamada EXATA será lida na doc/GIR da
   versão fixada de GTK4/WebKitGTK 6.0 num spike de ≤20 min; **não inventar API**.
2. **Compositor (fallback):** não compor o frame enquanto o buffer for menor que um limiar plausível
   (ex.: menor que `target`), evitando o escalonamento grotesco; efeito colateral: a janela pode ficar sem
   decoração por poucos frames — precisa ser medido e aceito.
Se a decisão 0 não for dada, PARE e pergunte. Nenhuma das opções mexe em protocolo/ADR.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste primeiro, visto falhando:** criar `tests/integration/frame_grab_configure_hygiene.py` para
   correlacionar `FRAME_CONFIGURE t=... source=...` com o instante do `press`/`release` sintéticos e assertar
   **ZERO configures entre press e release** (o `source=final` deve vir só no unset, após o release). Rodar no
   código atual e salvar baseline **FALHA** mostrando o `source=grab-start` cru. Nenhuma amostra descartada;
   3 execuções seguidas no fim. O teste antigo `frame_initial_buffer.py` fica como prova da causa já eliminada.
2. Aplicar a correção mínima em `crates/aeroshell-compositor/src/grab.rs` (UM arquivo): remover o
   `send_frame_configure(window_id, big, "grab-start")` e o cálculo de `big` em `start_geometry_grab`; manter
   `source=final` no `unset` e `resize_grab_active`. Nenhuma mudança em `target = content + extras`, `move`,
   conteúdo HTML, `set_titlebar_slices`, 9-slice da 02k/02l, protocolo, shell ou ADR 0006.
3. `python3 tests/integration/frame_resize_glue.py` 3× → `VEREDITO: OK`, com `title_n=3`,
   `declared=(120,96)`, `SLICE_GEOM` 9 fatias cobrindo o alvo, e **nenhum** frame com `buf=200x200`/
   `sx>2`. Vizinhos: `frame_configure_hygiene.py`, `frame_buttons.py`, `frame_follows_resize.py`,
   `frame_window_id.py` OK; `frame_resize_sync.py` segue FALHA.
4. `bash scripts/check.sh` → `OK: scripts/check.sh passou`;
   `cargo clippy --workspace --all-targets --features bench -- -D warnings` → EXIT=0.
5. **JUIZ VISUAL (humano):** executar o nested do RUNBOOK abaixo com 2 `foot`, deixar a aplicação **aberta**,
   abrir/clicar as bordas da 1ª e da 2ª janela (lenta e rapidamente) e olhar o instante do clique. O humano
   deve ver a moldura e a barra com tamanho estáveis desde o primeiro frame, sem o "encolhe/volta" de
   milissegundos. Só com o "ok" explícito o item fecha; caso contrário fica em STATE.md §7 (AGENTS.md §0.9).
6. Registrar números medidos em `docs/STATE.md` (ressalva VM sem 3D) e comitar localmente, sem push.

Comando manual do RUNBOOK (deixar aberto; quem fecha é o humano):
```
cargo build --workspace --examples
LD_PRELOAD=libgtk4-layer-shell.so.0 target/debug/aeroshell-compositor \
  --backend winit --socket wayland-aero-02m \
  --shell target/debug/examples/frame_webview \
  --shell-env LD_PRELOAD=libgtk4-layer-shell.so.0 \
  --command foot
# 2ª janela: WAYLAND_DISPLAY=wayland-aero-02m foot
```

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Reabrir 02c/02e/02f/02j/02k/02l por conta própria; mudar a 02l, o protocolo ou ADR 0006.
- Mudar `target = content + extras`, a política de move, as linhas de corte da titlebar ou o HTML da barra.
- Editar `AGENTS.md`, `rust-toolchain.toml`, `Cargo.lock`, `security-model.md`, `packaging/*`.
- Dependências novas (crate/npm/apt), WPE, Chromium/CEF, mudanças fora do repositório.
- Matar processo por nome genérico; encerrar instância aberta pelo humano; `sudo`.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem resultado visível → pare e descreva o bloqueio. Spike de API: ≤20 min, com fonte citada.
  Regra das duas tentativas: se a mesma abordagem falhar 2×, PARE e peça decisão (AGENTS.md §3.12 / §4.1).
- Decisão 0 é pré-requisito: sem ela, NÃO editar código.
- Nenhuma dependência nova. Arquivos protegidos: ver AGENTS.md §6.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais
(já anotadas). Ao final: atualizar STATE.md (§3, §4, §5, §6, §7, §9) e comitar localmente (sem push).
