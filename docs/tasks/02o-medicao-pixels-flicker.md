# Tarefa 02o — medição de pixels por frame no release (SOMENTE instrumentação)

> Autorizada pelo humano em 2026-11-20, depois do juiz negativo da 02n ("problema ainda ocorre").
> Nova abordagem: ANTES de qualquer correção, produzir evidência crua de pixels/frame real.
> Esta tarefa NÃO é a 5ª correção e NÃO muda comportamento do binário sem `bench`.

## Objetivo (uma frase, verificável)
Ao soltar um resize com arrasto, produzir dumps de pixels reais do framebuffer do compositor por frame
renderizado e uma métrica de diferença entre frames, respondendo com evidência crua se (a) o 9-slice e o
nativo diferem em pixels no mesmo target e (b) existe um transiente de pixels que corresponda ao flicker
relatado pelo humano — sem alterar press/arrasto, 9-slice, target, move, conteúdo, protocolo, shell, deps ou ADRs.

## Contexto mínimo (arquivos que importam)
- `docs/STATE.md §7` — juiz negou a 02n; o teste 02n mede SÓ estados (`rescaled`, tamanho do buffer, serial),
  nunca pixels. Essa é a lacuna de medição a fechar; nenhuma amostra antiga foi descartada.
- `tests/integration/frame_release_swap_hygiene.py:110-129,131-189` — harness de drag/release sintético; origem do
  frame 0 em `(54,50)` (INSETS 6/30/6/6 a partir de `(60,80)`), canto arrastado em `edge` calculado no script.
  Reutilizar a mesma sequência de input para que a medição seja comparável à 02n.
- `crates/aeroshell-compositor/src/backend/winit.rs:149-160,~572-596` — ponto após
  `renderer.render(&mut framebuffer, ...)` e antes/independente de `backend.submit`; é onde o readback entra.
- `crates/aeroshell-compositor/src/state.rs` — `ReleaseSwap`/`slice_generation` e constantes 200/50/400 ms (02n);
  os logs `RESIZE_SLICE` já carregam `t`, `buf`, `target`, `rescaled` para alinhar pixels e estados.
- API REAL da versão fixada (Smithay 0.7.0), sem dependência nova:
  - `ExportMem::copy_framebuffer(&mut self, &Framebuffer, Rectangle<i32, BufferCoord>, Fourcc) -> TextureMapping`
    — `/home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/smithay-0.7.0/src/backend/renderer/mod.rs:719-738`.
  - `ExportMem::map_texture(&mut self, &TextureMapping) -> Result<&[u8], Error>` — mesmo arquivo `:768-775`.
  - `Fourcc` já vem de `smithay::backend::allocator::Fourcc` (crate já é dependência);
    `Fourcc::Argb8888` é suportada pelo GLES — `.../renderer/gles/format.rs:20-21`.
- Captura externa NÃO serve aqui: `ImageGrab.grab()` → `OSError X get_image failed: error 8`; `ffmpeg`, `import`,
  `xwd`, `scrot`, `maim`, `grim` ausentes; Pillow instalada. Readback no compositor é a via escolhida.
- Instância visual da 02n está aberta: socket `wayland-aero-02n`, compositor PID 118815, `foot` PIDs 118897/118942.
  **NÃO encerrar.** A medição usa socket novo (`wayland-aero-02o`), em paralelo.

## Critério de aceite (comandos exatos + resultado esperado)
1. Instrumentação SOMENTE sob `cfg(feature = "bench")` com a variável `AERO_PIXEL_DUMP_DIR` (se vazia, nada é lido
   nem escrito). `cargo build -p aeroshell-compositor --features bench` → OK; `cargo build -p aeroshell-compositor`
   → OK e nenhuma chamada de readback no código fora de `cfg(feature="bench")` (conferir por `grep` no diff).
2. `python3 tests/integration/frame_release_pixel_probe.py` → cria `/tmp/opencode/02o_pixels/`, executa o mesmo
   drag/release do harness 02n, imprime TODAS as linhas cruas `PIXEL_FRAME t=<ns> frame=<n> buf=... target=...
   rescaled=<bool> gen=<n> regiao=<A|B> sha=<hex>` e termina com:
   `frames_capturados=N pares_9slice_nativo=M diff_medio=<f> diff_max=<f> flashes_candidatos=K
   VEREDITO_MEDICAO: CAPTUROU | NAO_CAPTUROU | PAR_NAO_CAPTURADO`.
   Nenhuma amostra descartada; linhas corrompidas por interleaving contadas como `malformadas=N`.
3. Métrica PRÉ-REGISTRADA (decidida aqui, antes de olhar os dados — não ajustar limiar depois):
   - Região A (barra de título da janela 0): output `(54,50)-(354,90)`, ajustada só pela escala real do output
     lida no log; região B (canto arrastado): `(edge_x-12, edge_y-12)-(edge_x+12, edge_y+12)`, com `edge` lido
     do harness. As duas regiões são descritas no cabeçalho do relatório.
   - `diff` = média de `|canal RGB|` por pixel entre dois dumps da mesma região.
   - `flash candidato` = frames consecutivos com `Δt ≤ 100 ms` e `≥10%` dos pixels da região A mudando `≥32/255`.
   - `PAR_9SLICE_NATIVO` = último frame `rescaled=true` com `buf==target` e o primeiro `rescaled=false` com
     `buf==target`. Se o `diff` entre eles for ~0, a hipótese "visualmente idênticos em tema uniforme" da 02n
     fica CONFIRMADA e essa troca NÃO é a causa; se for >0, fica REFUTADA e é evidência crua da causa.
4. Teste manual com juiz humano (obrigatório para correlacionar com a percepção): abrir a instância
   instrumentada no socket `wayland-aero-02o` com 2 `foot` pelo comando do RUNBOOK (com
   `AERO_PIXEL_DUMP_DIR=/tmp/opencode/02o_pixels_manual`), **deixar aberta**; humano clica, segura, arrasta e solta
   as bordas e diz se viu flicker **nesta execução instrumentada** (e se foi só no soltar ou também no clique/arrasto,
   dúvida ainda não respondida). Agente correlaciona o instante relatado com os `PIXEL_FRAME` medidos. Sem essa
   correlação, o cartão fica PARCIAL e registrado em `STATE.md §7`. Quem fecha a aplicação é o humano.
5. `bash scripts/check.sh` → `OK: scripts/check.sh passou` (bun 9 pass / 0 fail).
6. `cargo clippy --workspace --all-targets --features bench -- -D warnings` → EXIT=0.
7. Vizinhos sem mudança de resultado: `frame_release_swap_hygiene.py` (3× OK), `frame_configure_hygiene.py`,
   `frame_buttons.py`, `frame_window_id.py`, `frame_resize_glue.py`, `frame_follows_resize.py`.

## Fora de escopo (NÃO faça)
- Corrigir o flicker; abrir a 5ª correção; mexer em 02m (press/arrasto aprovado), 02k/02l (9-slice), `target =
  content + extras`, move, conteúdo HTML, protocolo (`protocols/*.xml`), shell, ADRs ou dependências.
- Alterar a 02n ou encerrar PIDs 118815/118897/118942 (instância aberta).

## Ressalvas a registrar no veredito
- Readback é síncrono e pode alterar o tempo dos frames: contar frames com e sem dump e registrar a diferença.
  VMs sem 3D: nada de conclusão de desempenho, só funcionamento e pixels relativos.
- Os pixels vêm do framebuffer do compositor (o que ele apresenta), não do sinal final do monitor. Se o humano vir
  flicker e a medição não capturar, registrar a lacuna e pedir decisão — NÃO declarar "sem flicker" (AGENTS.md §0.4).

## Limites
- Tempo: ~2 h sem nenhuma captura utilizável → pare e descreva o bloqueio.
- Regra das duas tentativas ativa; se a mesma abordagem de captura falhar 2×, pare e peça decisão.
- Dependências novas: nenhuma. Arquivos protegidos: ver AGENTS.md §6.

## Entrega
Relatório curto: o que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado ·
descobertas laterais (já anotadas em STATE.md §6). Ao final: atualizar STATE.md (§3, §4, §5, §6, §7, §9) e
comitar localmente (sem push). NÃO iniciar correção: esta tarefa termina com a medição e a decisão do humano.
