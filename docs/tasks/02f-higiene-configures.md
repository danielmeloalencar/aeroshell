# Tarefa 02f — higiene dos configures do frame durante resize

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
Num arrasto de borda/canto, o compositor envia no máximo 1 configure de decoração para cada 3 motions (razão
`FRAME_CONFIGURE/RESIZE_SYNC_MOTION ≤ 1/3` em 14 passos de 50 ms), não reenvia configure no commit-hook enquanto o
grab está ativo e termina com um único configure final no tamanho real do conteúdo.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-compositor/src/grab.rs:178-275` — `ResizeGrab::motion` chama `send_frame_configure` a CADA
  motion (`grab.rs:266-268`); `unset` hoje é vazio (`grab.rs:393`).
- `crates/aeroshell-compositor/src/protocols/compositor.rs:182-204` — o commit-hook reenvia configure quando o
  cliente comita um tamanho diferente do último configurado. Durante o grab isso "puxa" a moldura de volta ao
  tamanho atrasado do conteúdo.
- `crates/aeroshell-compositor/src/protocols/aero_frame.rs:59-101` — `send_frame_configure` (4 chamadores:
  `grab.rs:267`, `compositor.rs:200`, `aero_frame.rs:309`, `layer_shell.rs:69`). Precisa instrumentar
  `FRAME_CONFIGURE serial=<s> window_id=<id> w=<w> h=<h> source=<grab|commit|probe|final|layer>` sob `cfg(feature="bench")`.
- `crates/aeroshell-compositor/src/state.rs:73-123` — `AeroState`; adicionar sinal `resize_grab_active: Option<u32>`
  (window_id) para o commit-hook saber que o grab está ativo.
- `tests/integration/frame_resize_glue.py` — harness a copiar: build `--features bench`, socket de input sintético,
  14 passos de 50 ms, `FRAME_BUFFER_COMMIT`/`RESIZE_GLUE` para o settle.
- Evidência do problema: `/tmp/opencode/aero-02e-visual.log` — seriais de `frame configure` subiram 1→92 num único
  arrasto do juiz (STATE.md §7). ADR 0006 dá o contexto do pipeline WebKit; NÃO alterar.

## Critério de aceite (comandos exatos + resultado esperado)
1. `python3 tests/integration/frame_configure_hygiene.py` → `VEREDITO: OK`. O MESMO teste, rodado ANTES da correção,
   deve falhar com `configures/motions` > 1/3 e `source=commit` durante o grab (registrar a saída crua como prova).
   O teste conta `RESIZE_SYNC_MOTION` e `FRAME_CONFIGURE` no intervalo dos 14 passos e exige
   `configures(source=grab) <= motions/3`.
2. O mesmo teste exige: (a) `RESIZE_SYNC_CLIENT_CONFIGURE == RESIZE_SYNC_MOTION` — o configure do cliente continua
   em TODOS os motions; (b) nenhum `FRAME_CONFIGURE ... source=commit` enquanto o grab está ativo; (c) após soltar,
   exatamente UM `FRAME_CONFIGURE ... source=final` nos primeiros 100 ms, com `w,h` iguais ao último
   `RESIZE_SYNC_CLIENT size=<w>x<h>` anterior ao release; (d) `FRAME_BUFFER_COMMIT ... ok=true` e
   `RESIZE_GLUE ... rescaled=false` com `buffer==target` em até 500 ms após soltar.
3. `python3 tests/integration/frame_resize_glue.py` → `VEREDITO: OK` em 3 execuções seguidas, nenhuma amostra
   descartada. Vizinhos `frame_resize_edges.py`, `frame_follows_resize.py`, `frame_buttons.py`,
   `frame_window_id.py` → 4× OK. `frame_resize_sync.py` → `VEREDITO: FALHA` (métrica antiga; NÃO editar).
4. `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0).
   `cargo clippy --workspace --all-targets --features bench -- -D warnings` → EXIT=0.
5. Teste manual (juiz humano): abrir o nested com 2 `foot`, arrastar a borda direita e o canto inferior-direito,
   lento e rápido, em ambas as janelas; deixar a aplicação aberta. O humano deve dizer que a moldura acompanha o
   conteúdo **sem chegar atrasada**. Sem o "ok" do humano, 02f fica em `STATE.md` §7 como NÃO validada.

### Regras de implementação (necessárias para o critério 1)
- `ResizeGrab` guarda `last_sent_size: Option<(i32, i32)>` e `last_sent_at: Option<std::time::Instant>`.
- O configure da decoração só é enviado se o tamanho mudou E (é o primeiro passo OU passaram ≥150 ms do último).
  Escolher **200 ms** (satisfaz o limite inferior de 150 ms) para que 14 passos de 50 ms produzam 4 configures
  (4/14 ≈ 0,29 ≤ 1/3). Registrar a escolha no relatório; se o juiz visual piorar, reportar antes de mudar.
- O configure do cliente (`toplevel.send_configure()` em `grab.rs:271`) continua em TODOS os motions.
- `resize_grab_active` é preenchido em `start_geometry_grab` quando `resize.is_some()` e limpo em
  `ResizeGrab::unset`; o bloco de `compositor.rs:192-200` não chama `send_frame_configure` enquanto o sinal está ativo.
- `ResizeGrab::unset` envia UM `send_frame_configure(window_id, client_content_size(window_id), "final")`.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- 9-slice / `CropRenderElement`, mudanças na escala 02e, timer do shell (02g), medições de VM (02h), docs (02i),
  02d, 05, decisão de modo produto.
- Editar `protocols/*.xml`, ADRs aceitos, `docs/security-model.md`, `Cargo.lock`, `rust-toolchain.toml` ou
  `tests/integration/frame_resize_sync.py`.
- Alterar geometria, insets, shadow, damage ou o protocolo privado `aero_frame_v1`.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem redução mensurável na razão configure/motion → pare e descreva o bloqueio com dados crus.
- Dependências novas: NENHUMA. Arquivos protegidos: ver AGENTS.md §6.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §7, §9) e comitar localmente (sem push).
