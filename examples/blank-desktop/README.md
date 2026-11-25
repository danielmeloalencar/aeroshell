# blank-desktop

Background e painel declarados somente em `desktop.config.tsx`, sem framework e sem código Rust do desenvolvedor.

```bash
bun build packages/sdk/src/client.ts --target=browser --outfile=packages/sdk/dist/client.js
cargo build --workspace --examples
cargo build -p aeroshell-shell
AERO_WINDOW_IPC_SOCK=/tmp/opencode/blank-desktop-ipc.sock \
  bun packages/runtime/src/main.ts --config examples/blank-desktop/desktop.config.tsx
```

O painel importa `createClient()` de `./client.js` (o bundle genérico do SDK servido same-origin em `aero://surface/<id>/client.js`), assina eventos e atualiza somente suas regiões de input/blur. O mesmo cliente pode ser usado por React, Vue, Svelte ou Three.js sem adapter.

## Editar ao vivo (`--dev`)

Use a mesma origem loopback no compositor, no host e no runtime, todos iniciados com `AERO_DEV_ORIGIN=http://127.0.0.1:4173`. O runtime recebe adicionalmente `--dev`:

```bash
AERO_DEV_ORIGIN=http://127.0.0.1:4173 \
  bun packages/runtime/src/main.ts --config examples/blank-desktop/desktop.config.tsx --dev
```

Edite `desktop.config.tsx` para mudar markup/cores ou `panel.ts` para mudar o módulo. O primeiro arquivo gera diff IPC create/update/destroy; o segundo reconstrói o bundle e recarrega pelo WebSocket. Ambos preservam compositor, runtime, WebView e WebProcess. Sem `--dev`, `dev_entry` é ignorado em favor do HTML `aero://` e qualquer URL HTTP é recusada. Não use este servidor em produção.
