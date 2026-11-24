# Tarefa F2-surface-host-genérico — host de superfícies web genérico

> Cartão de tarefa: um agente deve conseguir executá-lo lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
O runtime TS cria, atualiza e destrói superfícies web `desktop|panel|overlay|frame` por comandos IPC, com z-order e região de entrada medidos, sem perder o spike `frame_webview` nem os harnesses atuais.

## Contexto mínimo (arquivos que importam)
- `docs/plano-mestre.md` §3/§4/§5 — contrato `aero.surfaces`, árvore e F2 como ponte web + SDK v0 + HMR.
- `docs/adr/0002` (WebKitGTK no host) e decisão ratificada de IPC textual + stream — NÃO reabrir.
- `crates/aeroshell-shell/src/` — novo módulo `surfaces/`; `frame_webview.rs` é o spike a migrar para o kind `frame`.
- `crates/aeroshell-compositor/src/window_ipc.rs` — comandos e stream PUSH existentes.
- `crates/aeroshell-ipc/src/types.rs` — tipos Rust→TS por ts-rs, nunca duplicados à mão.
- `crates/aeroshell-shell/src/scheme.rs` / `host.rs` — `aero://`, `shell-reset.css` e layer do painel.
- `tests/integration/frame_buttons.py`, `f1_csd_gtk.py`, `f2_full_decoration_policy.py`, `f2_workspaces.py`, `f2_launcher.py` — vizinhos obrigatórios.

## Escopo
1. Kinds `desktop|panel|overlay|frame` em `crates/aeroshell-shell/src/surfaces/`, dirigidos por comandos IPC do runtime: `surface-create {kind, output?, anchors, margins, exclusive_zone, layer, keyboard_mode, input_region, url|html}`, `surface-update`, `surface-destroy`.
2. Z-order §3: `desktop < [frame+cliente] < panels/overlay < lock`. LayerMap calcula geometry/exclusive zone. Blur-behind: a web só declara (ADR 0003), não implementar aqui.
3. Cada superfície = WebView com bridge padrão + `aero://` + `shell-reset.css`; pool/related-view e canvas fixo/9-slice do frame PRESERVADOS (ADR 0001/0007).
4. Tipos Rust→TS SOMENTE via ts-rs (`bun run generate:types`); eventos `SurfaceCreated/Updated/Destroyed` no stream.
5. Migrar o spike `frame_webview` para o kind `frame` do host sem perder harnesses.
6. Toda API pública nova com tipos + JSDoc + exemplo (regra 6).

## Critério de aceite (comandos exatos + resultado esperado)
1. **Baseline test-first:** `python3 tests/integration/f2_surface_host.py` → `VEREDITO: FALHA` antes da implementação, por comandos/formatos/eventos ausentes; depois → `VEREDITO: OK`, criando um background desktop sólido e um overlay com região transparente, medindo pixels/z-order e confirmando que um clique na área transparente chega ao cliente.
2. `bun run generate:types` → exit 0; `bun test` → verde com testes novos para comandos/eventos tipados.
3. Vizinhos de frame/CSD/policy/workspaces/launcher → `VEREDITO: OK`; falha é preservada e comparada com o HEAD antes de atribuir regressão.
4. `bash scripts/check.sh` → `OK: scripts/check.sh passou`; `cargo clippy --workspace --all-targets --features bench -- -D warnings` → exit 0; `cargo check --workspace --no-default-features` → exit 0.
5. **Teste manual (juiz humano):** nested canônico sem `--size`: background desktop, overlay transparente, 2 `foot`, z-order correto e clique passando pela área transparente. O humano responde explicitamente **“ok/aprovado”**; a aplicação fica aberta para ele fechar.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Lock/greeter (F5/F6), HMR, adapters, templates e layouts (F3).
- Serviços, incluindo notificações D-Bus (F4).
- Snap/F7.
- Recriar `aeroshell-web`/WPE ou msgpack sem ADR.
- Novas dependências sem aprovação.
- Arquivos protegidos: ver AGENTS.md §6.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem resultado visível → pare e descreva o bloqueio.
- Dependências novas: não.
- Commits: um por etapa validada, convenção `tipo(escopo): descrição — gate <id> OK`; sem push sem confirmação.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §6, §7, §9), obter o veredito visual e comitar localmente (sem push).
