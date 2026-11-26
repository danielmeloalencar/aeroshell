# win11-like (Vue)

Taskbar centrada e menu Iniciar em Vue, usando `useWindows`, `useApps`, `useWorkspaces`, `useOutputs` e `useEvent`. O menu vive numa Overlay fullscreen própria: a taskbar tem só 72 px e um menu dentro dela ficava cortado. `Super+3` e o botão da taskbar alternam o menu via `BroadcastChannel` same-origin; focus/launch continuam privilegiados no runtime Bun e não são fingidos no guest.

```bash
AERO_DEV_ORIGIN=http://127.0.0.1:4173 \
  bun packages/runtime/src/main.ts --config examples/win11-like/desktop.config.tsx --dev
```

Edite `panel.ts` para HMR ou remova `--dev` para usar o bundle embutido em `aero://`. O cartão/taskbar é opaco; o restante da layer é transparente e usa o hit-test por alfa.
