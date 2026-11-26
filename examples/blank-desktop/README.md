# blank-desktop (React)

Background e painéis nos outputs 0/1 declarados em `desktop.config.tsx`, sem Rust e sem IPC privilegiado no guest. O painel é React: `panel.ts` usa `mountSurface`, `useWindows` e `useEvent` do `@aeroshell/react`; o mesmo entry é empacotado em produção no HTML `aero://` e servido pelo dev-server em `--dev`. `Super+1`/`Super+2` ativam workspaces pelas ações tipadas do runtime.

```bash
cargo build --workspace --examples
cargo build -p aeroshell-shell
AERO_WINDOW_IPC_SOCK=/tmp/opencode/blank-desktop-ipc.sock \
  bun packages/runtime/src/main.ts --config examples/blank-desktop/desktop.config.tsx
```

O mount cria uma `SurfaceSession`: um único `createClient()` alimenta o `WindowStore`, o `ready()` dispara o replay existente e o documento não perde estado silenciosamente após HMR. O guest atualiza somente input/blur da própria superfície; `focus/launch` continuam no runtime Bun.

## Editar ao vivo (`--dev`)

Use a mesma origem loopback no compositor, no host e no runtime:

```bash
AERO_DEV_ORIGIN=http://127.0.0.1:4173 \
  bun packages/runtime/src/main.ts --config examples/blank-desktop/desktop.config.tsx --dev
```

Edite `desktop.config.tsx` para trocar cor/texto ou `panel.ts` para alterar o componente React. O config gera diff IPC create/update/destroy; o módulo gera reload via WebSocket. Compositor, runtime, WebView e WebProcess são preservados. Sem `--dev`, o bundle React fica inline em `aero://` e HTTP é recusado.

## Janelas pelo SDK privilegiado

```ts
const aero = createAero(ipcTransport(process.env.AERO_WINDOW_IPC_SOCK!));
await aero.windows.maximize(0);
await aero.windows.minimize(0);
await aero.windows.focus(0);
```
