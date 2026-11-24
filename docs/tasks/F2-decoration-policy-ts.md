# Tarefa F2-decoration-policy-ts — política completa de decoração em TS (RASCUNHO aprovado pelo humano)

> Terceiro cartão da F2. `F2-decoration-policy` FECHADA (`40e36b6`, juiz `b6c9613`): o **mecanismo** já existe
> (`DecorationRequest`/`DecorationApplied`, `policy-ready`, `policy <id> server|client|none`, fallback imediato sem
> runtime, timeout só com runtime, precedência política > CSD > default). Este cartão entrega a **política**:
> regras declarativas escritas em TS e executadas por um runtime Bun real, sem regra de app_id no compositor.
> F1 permanece fechada: nenhum protocolo Wayland/XML/ADR novo, nenhuma geometria/9-slice reaberta.

## Objetivo (uma frase, verificável)
Um runtime Bun carrega regras declarativas tipadas (tipos gerados por ts-rs) no startup e decide `server|client|none`
por `app_id`/classe com `default`, com o compositor sem nenhum app_id hardcoded além do fallback/CSD-detecção —
provado por teste-first, gates, vizinhos e juiz visual.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-ipc/src/types.rs` — `DecorationPolicy`/`AeroEvent` já são a fonte ts-rs; adicionar tipos de
  configuração da regra (`DecorationRule`, `DecorationPolicyConfig`) para o TS **não** declarar tipos à mão.
- `packages/sdk/src/generated/*.ts` — bindings gerados por `bun run generate:types` (`DecorationPolicy` é
  `"Server"|"Client"|"None"` na wire TS; o IPC textual usa minúsculas — conversão explícita e tipada no runtime).
- `packages/sdk/src/index.ts:33` (`parseEvent`) + `packages/runtime/src/ipc-client.ts` (`eventStream`, `ipcTransport`)
  — canal pergunta→resposta já existe e é reutilizado; **sem polling novo**.
- `packages/runtime/src/main.ts` — CLI atual (`--list/--focus/--close`); política completa entra como entrada própria.
- `crates/aeroshell-compositor/src/window_ipc.rs:363` / `state.rs:340-460` — mecanismo pronto; **não** alterar regras.
- `tests/integration/f2_decoration_policy.py` — padrão de harness (compositor headless + `policy_client` + sonda
  `POLICY` sob `bench`) reaproveitado, agora com runtime Bun de verdade.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste-first (unitário):** `bun test packages/runtime/src/decoration-policy.test.ts` escrito ANTES da
   implementação → falha por módulo ausente; depois verde: `gedit→Client`, `foot→Server`, `aero-no-deco→None`,
   desconhecido→`default`, primeira regra que casa vence, conversão `Server|Client|None` → `server|client|none`.
2. **Teste-first (integração):** `python3 tests/integration/f2_full_decoration_policy.py` escrito antes do runtime →
   baseline `VEREDITO: FALHA`. Depois `VEREDITO: OK` em 3 execuções com runtime Bun real: decisões `client/server/none`
   honradas por app_id/classe, `POLICY_RENDER suppressed=policy_none` para `none`, default honrado, 0 erros de
   protocolo; sem runtime → `reason=no-runtime` em <100 ms (nenhum timer no caminho frio); runtime conectado mas
   silencioso → `reason=timeout`.
3. **Sem regra hardcoded:** `grep` no `crates/aeroshell-compositor/src` não encontra `gedit|foot|aero-no-deco`
   fora de comentários (a política vive só em `packages/runtime/src/rules.ts`). Precedência política > CSD > default
   preservada (vizinhos CSD/frame abaixo).
4. **Regra editada em TS sem recompilar Rust:** o harness troca só um arquivo `.ts` de regras (via
   `AERO_DECORATION_RULES`, importado no startup) e reinicia somente o runtime Bun; a nova decisão aparece no log
   `POLICY … fallback=false` com o **mesmo binário** do compositor.
5. **Hot-reload FORA de escopo:** a configuração é lida uma vez no startup; nenhum watcher, nenhum recarregamento de
   regra de janela já decidida.
6. Gates: `bash scripts/check.sh` verde; `cargo clippy --workspace --all-targets --features bench -- -D warnings` 0;
   `cargo check --workspace --no-default-features` 0; `bun run generate:types` e `bun test` verdes; sonda `POLICY`
   e `POLICY_TS` ausentes do binário normal (`strings`).
7. Vizinhos (nenhuma regressão): `f1_csd_gtk.py`, `f1_csd_gtk.py --geometry-only`, `frame_title.py`,
   `f1_toplevel_icon.py`, `frame_buttons.py`, `frame_window_id.py`, `frame_resize_glue.py`,
   `headless_render_damage.py` → OK (ou flake pré-existente documentado em §6 com amostra preservada).
8. **Juiz visual (humano, sentença final):** instância nested deixada aberta com `foot` decorado, `gedit` sem
   barra dupla/flash, `aero-no-deco` sem moldura, controles do frame funcionando, e uma regra nova editada em TS
   (sem recompilar Rust) aplicada numa janela recém-aberta. Agente PARA no "ok".

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Hot-reload/watch de arquivos; mudar decisão de janela já decidida; S4/persistência de configuração.
- Experimento do snap (`F7-compat-snap.md`), F1, `protocols/*.xml`, ADR, dependência nova, `sudo`/`/etc`.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE. Máx. 15 min de desvio.

## Limites
- Tempo: 2 h sem política funcional → PARAR e reportar com dados; sem terceira via por conta própria.
- Dependências novas: nenhuma. Arquivos protegidos: ver `AGENTS.md §6`.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §6, §9) e comitar localmente (sem push). **PARAR para o ok visual.**
