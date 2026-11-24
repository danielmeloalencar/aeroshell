# Tarefa F2-decoration-policy-hotreload — hot-reload das regras TS de decoração (RASCUNHO — aguardando ok do humano)

> Quarto cartão da F2. `F2-decoration-policy-ts` FECHADA (`fcc07ef`, juiz `8fd4bdd`): as regras vivem em
> `packages/runtime/src/rules.ts` (ou `AERO_DECORATION_RULES`) e são lidas **uma vez no startup**. Este cartão
> entrega a atualização das regras **em disco, com o compositor e o runtime já rodando**, inclusive para janelas
> cuja política já foi decidida. F1 permanece fechada: nenhum protocolo Wayland/XML/ADR novo, nenhuma geometria
> reaberta, nenhuma dependência nova.

## Objetivo (uma frase, verificável)
Editar o `.ts` de regras em disco faz o runtime Bun reparsear e reaplicar a decisão em TODAS as janelas já
decididas, sem reiniciar compositor nem runtime; erro de sintaxe/configuração inválida mantém as últimas regras
boas, loga o erro e não envia nenhuma decisão — provado por teste-first, gates, vizinhos e juiz visual.

## Contexto mínimo (arquivos que importam)
- `packages/runtime/src/policy.ts:20-33` — hoje faz `await import(...)` UMA vez; vira loader + watcher.
- `packages/runtime/src/decoration-policy.ts` — avaliador puro e `DECORATION_WIRE` (`Server|Client|None` → wire).
- `packages/runtime/src/rules.ts` — arquivo de regras padrão; a edição ao vivo usa `AERO_DECORATION_RULES`.
- `packages/sdk/src/generated/DecorationPolicy{Config,Rule}.ts` — tipos ts-rs; validação em runtime precisa
  conferir valores (tipos TS desaparecem em runtime).
- `crates/aeroshell-compositor/src/state.rs:357-392` — `request_decoration_policy` tem guard
  (`decoration(id).is_some() || pending_policies`) que HOJE impede reaplicar numa janela já decidida.
- `crates/aeroshell-compositor/src/window_ipc.rs:349-384` — comandos internos no mesmo socket (`policy-ready`,
  `policy <id> server|client|none`); o reload deve **reaproveitar** esse caminho, não criar segunda rota.
  Desenho proposto: comando interno `policy-reload` faz o compositor limpar a decisão e reemitir
  `DecorationRequest` para as janelas existentes; o runtime responde com o `policy <id>` já existente.
- `tests/integration/f2_full_decoration_policy.py` — padrão de harness (headless + `bench` + `policy_client`).
- Bun 1.4.4 neste ambiente: `node:fs.watch` existe e foi sondado nesta sessão (`typeof watch === "function"`,
  evento `rename` disparado). Nenhuma dependência nova.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste-first unitário:** escrever `packages/runtime/src/policy-reload.test.ts` ANTES do módulo; baseline
   falha por módulo ausente. Depois `bun test packages/runtime/src/policy-reload.test.ts` → verde:
   config válida aceita; `default` inválido, `rules` não-array, `policy` desconhecida e `app_id`/`class` fora de
   `string|null` rejeitados; erro de sintaxe no `.ts` devolve falha **sem substituir** o último config bom.
2. **Teste-first integração:** escrever `tests/integration/f2_hotreload_decoration_policy.py` ANTES do
   watcher → baseline `VEREDITO: FALHA` (editar o `.ts` não muda nada); implementado, `VEREDITO: OK` em **3
   execuções**. Cada execução usa um `AERO_DECORATION_RULES` temporário e um `policy_client` sintético vivo:
   - estado inicial `aero-hotreload → Server`: `POLICY_TS RULES` + `POLICY … fallback=false` e frame presente;
   - editar o `.ts` para `None` **com compositor e runtime de pé**: aparece `POLICY_TS RELOAD` (nova decisão
     `id=… policy=none`), `POLICY … decision=none fallback=false` e `POLICY_RENDER suppressed=policy_none`
     **na mesma janela**; mesmo PID do compositor e mesmo PID do Bun antes/depois;
   - escrever sintaxe inválida: `POLICY_TS RELOAD ERROR` no log do runtime, **nenhuma** `DECISION` nem
     `policy` nova, nenhuma mudança de `decision`/`suppressed`; runtime continua vivo;
   - corrigir de volta para `Server`: `POLICY … decision=server fallback=false` na janela existente e
     `suppressed=policy_none` cessa;
   - `zero erros de protocolo` (`ProtocolError` ausente) e nenhum `reason=timeout` neste cenário.
3. **Regressão do mecanismo:** `python3 tests/integration/f2_full_decoration_policy.py` → `VEREDITO: OK`
   (4 casos originais preservados, cliente sintéticos com `AERO_POLICY_CLIENT_SECS` intacto).
4. **Sem regra hardcoded e sondas:** `grep -R 'gedit\|foot\|aero-no-deco' crates/aeroshell-compositor/src`
   continua sem resultado fora de comentários; `strings target/debug/aeroshell-compositor | grep POLICY_TS`
   → vazio (logs `POLICY_TS RELOAD` ficam só no runtime TS; qualquer sonda nova no Rust, se houver, sob
   `#[cfg(feature = "bench")]`).
5. **Gates:** `bash scripts/check.sh` → EXIT=0; `cargo clippy --workspace --all-targets --features bench -- -D
   warnings` → 0; `cargo check --workspace --no-default-features` → 0; `bun run generate:types` → 0;
   `bun test` → verde.
6. **Vizinhos (nenhuma regressão):** `f1_csd_gtk.py`, `f1_csd_gtk.py --geometry-only`, `frame_title.py`,
   `f1_toplevel_icon.py` (com binário `bench`), `frame_buttons.py`, `frame_window_id.py`,
   `frame_resize_glue.py`, `headless_render_damage.py` → OK, ou flake pré-existente documentado em §6 com a
   amostra preservada (sem descartar medição).
7. **Juiz visual (humano, sentença final):** instância nested deixada **aberta**, com `gedit` e runtime apontando
   para um `.ts` temporário. O agente edita a regra de `gedit` ao vivo de `Client` → `Server` → `Client` e
   escreve uma versão com erro de sintaxe; o humano confirma que a moldura do compositor aparece e desaparece no
   **mesmo `gedit` sem reiniciar nada**, que não há barra dupla no estado `Client`, e que o erro de sintaxe não
   altera a tela. Agente PARA no "ok"; quem fecha a instância é o humano.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Experimento do snap / F7, F1, `protocols/*.xml`, ADR aceito, segurança (lock/PAM/filtro), dependência nova,
  `sudo`/`/etc`, persistência de configuração, S4, hot-reload de código Rust, ícones/insets/geometria.
- Não reiniciar compositor nem runtime durante o teste de reload; não trocar a rota `policy <id>` existente por
  outra.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE. Máx. 15 min de desvio; regra das
duas tentativas.

## Limites
- Tempo: ~2 h sem reload funcional → PARAR e reportar com dados, sem terceira via por conta própria.
- Dependências novas: nenhuma (`node:fs` já vem com Bun). Arquivos protegidos: ver `AGENTS.md §6`.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar `docs/STATE.md` (§3, §4, §5, §6, §7, §9) e comitar localmente (sem push).
**PARAR para o ok visual.**
