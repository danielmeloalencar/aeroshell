# Tarefa F2-shell-real — painel React de produção + regras persistentes no config dir (PARCIAL — dívida de pintura aceita pelo humano)

> Próximo cartão da F2, após `F2-decoration-policy-hotreload` FECHADO (`b532a0c`, juiz: "ocorreu tudo ok").
> O painel já existe (`packages/panel/src/index.tsx`, servido em `aero://panel/app.js` via
> `crates/aeroshell-shell/src/scheme.rs:11`) e consome o stream por `__aeroEvent`; este cartão o torna o
> **painel de produção completo** (abrir/fechar/focar + indicadores) e resolve a persistência pendente das
> regras (hoje lidas de `packages/runtime/src/rules.ts` ou `AERO_DECORATION_RULES`, uma vez no startup).
> Nenhum protocolo/ADR/XML novo, nenhuma dependência nova, F1/snap/F7 intocados.

## Objetivo (uma frase, verificável)
O painel React servido por `aero://` mantém a lista de janelas ao vivo **somente pelo stream `AeroEvent`**
(sem poll), com botão de fechar e indicadores de foco/decoração; as regras de decoração passam a ser lidas
de `~/.config/aeroshell/rules.ts` (sobrepostas por `AERO_DECORATION_RULES`), sobrevivem a restart e continuam
com hot-reload por `fs.watch` — provado por teste-first, gates, vizinhos e juiz visual com 2–3 apps reais.

## Contexto mínimo (arquivos que importam)
- `packages/panel/src/index.tsx:38-49` — transporte é a ponte `window.webkit.messageHandlers.aero`; o
  painel já usa `createAero(transport)` + `window.__aeroEvent → aero.store.applyEvent`. Adicionar ✕ (close),
  indicadores e publicar estado renderizado para o harness.
- `packages/panel/dist/app.js` — bundle gerado por
  `bun build packages/panel/src/index.tsx --target=browser --outfile=packages/panel/dist/app.js`;
  é `include_bytes!` em `crates/aeroshell-shell/src/scheme.rs:11` (produção, sem dev server).
- `packages/sdk/src/index.ts:54-94` — `createAero`: `windows.list/focus/close` sobre o transporte tipado.
- `packages/sdk/src/store.ts:57-86` — `applyEvent` hoje trata `Snapshot/Title/Geometry` e **ignora**
  `Focus`/`DecorationApplied`; estender para carregar os indicadores (testes em `store.test.ts`).
- `packages/sdk/src/generated/AeroEvent.ts` — tipos ts-rs (nunca editar à mão; `bun run generate:types`).
- `packages/runtime/src/policy.ts:24-31,67-78` — resolve o caminho das regras e faz `fs.watch` no diretório;
  a resolução passa a incluir o config dir, mantendo `AERO_DECORATION_RULES` como override.
- `packages/runtime/src/policy-config.ts` — `loadRules`/`RuleSource` (validação + últimas boas) reaproveitados.
- `tests/integration/react_panel.py:105-128` — padrão de leitura do painel pelo log (`BRIDGE aeroEvent` e
  `mensagem da WebView`); base para o harness novo.
- `crates/aeroshell-shell/src/bridge.rs:141-176` — comandos `ipc` (list/focus/close) e mensagens da WebView
  já são logadas; sem protocolo novo.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste-first unitário (store):** escrever testes novos em `packages/sdk/src/store.test.ts` ANTES da
   extensão → baseline falha. Depois `bun test packages/sdk/src/store.test.ts` verde: `Focus` marca a janela
   ativa, `DecorationApplied` guarda `server|client|none` por id, patches com revisão antiga continuam
   ignorados. `bun test` → 20+ testes verdes.
2. **Regras no config dir:** com `HOME`/`XDG_CONFIG_HOME` temporários (harness), escrever
   `$XDG_CONFIG_HOME/aeroshell/rules.ts` e iniciar compositor + runtime **sem** `AERO_DECORATION_RULES`:
   `POLICY_TS RULES source=config-dir …` e a decisão do cliente sintético reflete o arquivo. `AERO_DECORATION_RULES`
   continua vencendo (`source=edited-ts`). Sem arquivo algum, cai para o bundle (`source=default`), sem regressão
   nos harnesses existentes. Após kill/restart com o mesmo config dir, a decisão volta **sem reescrever nada**.
3. **Teste-first integração (novo harness):** escrever `tests/integration/f2_shell_real.py` ANTES do painel
   novo → baseline `VEREDITO: FALHA` (close/indicadores/config-dir ausentes). Implementado, `VEREDITO: OK`
   em **3 execuções**. Cada execução usa headless + `bench` + `policy_client` sintético + o `aeroshell-shell`
   real, e verifica:
   - subir 2 `foot` → o painel publica 2 linhas ao vivo (id + título) vindas de `BRIDGE aeroEvent … Snapshot`,
     sem nenhum `setInterval` de `list` no bundle (`grep` no `dist/app.js`);
   - abrir um 3º cliente → aparece a 3ª linha sem restart; fechar um → a linha desaparece (evento `Remove`/snapshot
     PUSH), mesmo PID do shell;
   - clicar numa linha eleva só aquela janela (`ordem de render` muda, padrão de `react_panel.py`);
   - a linha ativa traz indicador de foco e o indicador de decoração bate com `DecorationApplied`;
   - editar `$XDG_CONFIG_HOME/aeroshell/rules.ts` (`Server → None` e volta) → nova decisão na **mesma janela**,
     com `POLICY_TS RELOAD` e `POLICY_RENDER suppressed=policy_none` alternando, sem restart; erro de sintaxe
     mantém as últimas boas (`RELOAD ERROR`, 0 decisões novas, runtime vivo);
   - 0 erros de protocolo e nenhum `reason=timeout` neste cenário.
4. **Reservas do cartão anterior medidas aqui (não em cartão novo):** o reload com **múltiplas janelas**
   (2–3 decididas ao mesmo tempo) atualiza todas; medir a **latência do reload** (tempo entre a escrita do
   `.ts` e `POLICY_TS RELOAD_DISPATCH`) em 3 amostras, registrando que é VM sem 3D (valor relativo).
5. **Regressão:** `python3 tests/integration/react_panel.py` (2 e 3 `foot`) e
   `python3 tests/integration/f2_full_decoration_policy.py` → `VEREDITO: OK`.
6. **Sem API interna / sondas:** o painel usa só `createAero` + `parseEvent`/tipos gerados; nenhuma string
   `W ` é parseada dentro de `index.tsx`, nenhuma referência a `crates/aeroshell-compositor`. `grep` em
   `crates/aeroshell-compositor/src` continua sem app_id literal. Nenhuma sonda nova fora de `bench`.
7. **Gates:** `bash scripts/check.sh` → EXIT=0; `cargo clippy --workspace --all-targets --features bench -- -D
   warnings` → 0; `cargo check --workspace --no-default-features` → 0; `bun run generate:types` → 0;
   `bun test` → verde. `bun build …` gera o bundle sem erro e o `aero://panel/index.html` o serve.
8. **Vizinhos:** `f1_csd_gtk.py`, `f1_csd_gtk.py --geometry-only`, `frame_title.py`, `f1_toplevel_icon.py`
   (com binário `bench`), `frame_buttons.py`, `frame_window_id.py`, `frame_resize_glue.py`,
   `headless_render_damage.py` → OK, ou flake pré-existente documentado em §6 com a amostra preservada.
9. **Juiz visual (humano, sentença final):** instância nested deixada **aberta** com o painel e 2–3 apps
   reais (`foot` + `gedit`). O agente: (a) abre/fecha uma janela e mostra a lista mudando sozinha; (b) clica
   numa linha e mostra só aquela janela subindo; (c) edita o `rules.ts` do config dir e mostra a decoração
   mudando no mesmo app, sem restart; (d) escreve sintaxe inválida e mostra que nada muda. O humano confirma
   cada item. Agente PARA no "ok"; quem fecha a instância é o humano.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Experimento do snap / F7, F1, `protocols/*.xml`, ADR aceito, segurança (lock/PAM/filtro de globais),
  dependência nova, `sudo`/`/etc` (exceto escrita no config dir do **usuário** sob `$XDG_CONFIG_HOME` temporário
  do harness), git push.
- Não reabrir geometria/insets/ícones de frame, não criar protocolo Wayland novo, não trocar a rota IPC
  `policy <id>`, não mexer em lock/greeter, workspaces/layouts ou indicadores de sistema (relógio/bateria).
  "Indicadores" aqui significam **estado por janela**: foco e decoração.
- Não trocar o transporte da WebView (a ponte `window.webkit.messageHandlers.aero` é o limite do SDK, não
  uma API interna do compositor).

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE. Máx. 15 min de desvio; regra das
duas tentativas.

## Limites
- Tempo: ~2 h sem painel funcional → PARAR e reportar com dados, sem terceira via por conta própria.
- Dependências novas: nenhuma (React/react-dom 19.2.0 e `bun build` já aprovados). Arquivos protegidos:
  ver `AGENTS.md §6`.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar `docs/STATE.md` (§3, §4, §5, §6, §7, §9) e comitar localmente (sem push).
**PARAR para o ok visual.**
