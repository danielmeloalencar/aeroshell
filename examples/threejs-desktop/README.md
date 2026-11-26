# threejs-desktop (Three.js + Solid)

`background.js` cria uma cena WebGL procedural com `three@0.187.0`, sem assets externos. `widgets.tsx` monta um widget Solid reativo. O botão ou `Super+4` cancela/inicia o `requestAnimationFrame`; parado, não há redraw contínuo. A região de input cobre a overlay inteira para que o alfa real decida: cartão opaco captura, transparência repassa ao cliente.

```bash
AERO_DEV_ORIGIN=http://127.0.0.1:4173 \
  bun packages/runtime/src/main.ts --config examples/threejs-desktop/desktop.config.tsx --dev
```

A dependência `three` pertence somente a este exemplo. Em produção o runtime embute o mesmo entry no HTML `aero://`, sem dev server.
