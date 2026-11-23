# Tarefa F2-event-stream — stream de eventos tipados (S4 deferido) (RASCUNHO — aguarda "ok")

> Primeiro cartão da F2 (ponte web + SDK + shell real). F1 FECHADA em `585fa6f`; não reabrir protocolos de núcleo.
> S4 foi deferido no fechamento da F0 (`docs/f0-relatorio-go-no-go.md`); este cartão entrega o **mecanismo**, não a
> política completa de decoração (cartão F2 seguinte).

## Objetivo (uma frase, verificável)
Substituir o IPC request/response com polling (100 ms/1 s) por um stream push de eventos tipados do compositor para o
shell/SDK — `window list/title/icon/focus/geometry` e `frame_painted` — com bindings TypeScript gerados por ts-rs e
re-render do React por evento, provado por teste-first e juiz visual.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-ipc/src/types.rs:13-50` — `WindowInfo`, `Snapshot`, `WindowChange`, `Patch` já derivam `TS`
  sob `export-bindings`; **nova fonte dos bindings TS**, nunca escrever interfaces à mão.
- `package.json:13` — `bun run generate:types` = `cargo test -p aeroshell-ipc --features export-bindings`.
- `crates/aeroshell-compositor/src/window_ipc.rs:120-...` — hoje `list/focus/close/maximize/icon/csd` são
  request/response por conexão curta; adicionar assinatura/stream **sem quebrar** os comandos atuais (F1 fechada).
- `crates/aeroshell-shell/examples/frame_webview.rs:683,823,1502,1508,1514` — `refresh_titles()` (1 s),
  `refresh_csd()` (100 ms) e polling de 20 ms são o que deve desaparecer; `ipc_title()`/`ICON` viram eventos.
- `crates/aeroshell-shell/src/bridge.rs:18-38,79-104` — `forward()` conecta por comando e responde `__aeroReply(seq,…)`;
  virá também consumidor do stream (`window.__aeroEvent(event)`).
- `docs/STATE.md §3/§7` — F1 provou título/ícone/CSD por polling; este cartão troca só o MECHANISMO de entrega.
- ADRs: 0004 (frame/layer surfaces) e 0007 (visibilidade/target) intactos; nenhum protocolo `protocols/*.xml` muda.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste-first:** `tests/integration/f2_event_stream.py` escrito antes; com o código atual (sem push) →
   `VEREDITO: FALHA` (shell/WebView não recebe nenhum evento, só responde a `list`).
2. Compositor emite eventos **no ponto de mudança** (map/unmap, `title`, foco, `configure`/geometria, ícone,
   `frame_painted`) numa conexão de stream; log bench `EVENT_PUSH t=… kind=… window_id=…` e nenhum evento no commit
   de rotina sem mudança. `strings` do binário sem `bench` → 0 ocorrências da sonda.
3. `bun run generate:types` → bindings TS gerados (somente arquivos de `bindings/`, nunca editados à mão) e
   `bun test` verde, com teste do store aplicando eventos/patches revisionados (revisão <= atual ignorada).
4. Shell/SDK consomem o stream sem `timeout_add_local` de 100 ms/1 s para título/CSD/lista; React re-renderiza por
   evento. Comandos de volta (`close/maximize/focus`) continuam funcionando pelo canal de request existente.
5. Juiz visual (humano): com `foot` aberto, trocar `-T <título>` e o ícone ao vivo atualizam a titlebar **sem
   polling**; maximize/foco refletem no painel; 0 erros de protocolo no log.
6. Gates: `bash scripts/check.sh` verde; `cargo clippy --workspace --all-targets --features bench -- -D warnings`
   EXIT=0; `cargo check --workspace --no-default-features` EXIT=0.
7. Vizinhos de frame: `frame_title.py`, `f1_toplevel_icon.py`, `frame_buttons.py`, `frame_window_id.py`,
   `frame_resize_glue.py`, `headless_render_damage.py` → OK (ou falha pré-existente documentada com A/B).
8. **Gate de sessão F2 (tty3, humano opera):** rodar Firefox-snap **como sessão** (passo 1 de
   `docs/tasks/F7-compat-snap.md`). Se abrir → reduzir aquele cartão para doc + socket padrão; se negar → manter
   `F7-compat-snap` com o dado cru. **Não fazer snapd/slot neste cartão.**

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Política completa de decoração CSD/app_id (cartão F2 seguinte): este cartão só entrega o mecanismo de eventos.
- Reabrir geometria/9-slice/protocolos da F1; mudar `protocols/*.xml`, ADR aceito ou dependência nova.
- `snap connect`, registrar slot Wayland, mexer em `/etc` ou fora do repositório (AGENTS §3.8).
- F3+ (shell real completo), lock/PAM/greeter.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md §6` (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE. Máx. 15 min de desvio.

## Limites
- Tempo: 2 h sem stream funcional ao shell → PARAR e reportar com dados; sem terceira via por conta própria.
- Dependências novas: nenhuma (ts-rs/serde_json já presentes); arquivos protegidos: ver AGENTS.md §6.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §6, §9) e comitar localmente (sem push). **PARAR para o ok visual.**
