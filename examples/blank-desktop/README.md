# blank-desktop

Background e painéis nos outputs 0/1 declarados somente em `desktop.config.tsx`, sem framework e sem código Rust do desenvolvedor. `Super+1` e `Super+2` ativam os workspaces 1/2 por ações tipadas do SDK (crie o workspace 2 antes de usar o segundo atalho).

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

## Janelas pelo SDK privilegiado

`aero.windows` roda em Bun com acesso ao IPC, nunca dentro da WebView guest. O store é local: conecte o stream para receber `Minimized`, `Maximized`, `Fullscreen` e `Geometry` sem polling.

```ts
import { createAero, parseEvent } from "../../packages/sdk/src/index";
import { eventStream, ipcTransport } from "../../packages/runtime/src/ipc-client";

const path = process.env.AERO_WINDOW_IPC_SOCK!;
const aero = createAero(ipcTransport(path));
const disconnect = eventStream(path, raw => {
  const event = parseEvent(raw);
  if (event) aero.store.applyEvent(event);
});
const unsubscribe = aero.store.subscribeWindowStates(() => {
  console.log(aero.store.getWindowState(0));
});

await aero.windows.move(0, 700, 100);       // coordenadas globais lógicas
await aero.windows.resize(0, 500, 300);     // tamanho do cliente, sem insets
await aero.windows.maximize(0);             // work area do output sob a janela
await aero.windows.minimize(0);             // preserva workspace e membership
await aero.windows.focus(0);                // restaura e foca a mesma janela
await aero.windows.fullscreen(0);           // output inteiro; repita para restaurar

// Somente com o botão primário pressionado; o release encerra o grab Rust:
await aero.windows.beginMove(0);
await aero.windows.beginResize(0, "BottomRight");

await aero.windows.thumbnail(0);            // rejeita: screencopy/F7
unsubscribe();
disconnect();
```
