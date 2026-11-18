# ADR 0002 — Versões fixadas e licenças

- Status: aceito (F0)
- Data: 2026-11-18

## Contexto
A arquitetura depende de Smithay, WebKitGTK/GTK4, gtk4-layer-shell, PAM, greetd e MessagePack.
Versões precisam ser fixadas e APIs precisam ser consultadas, não inventadas (regra 3).

## Decisões
1. `smithay = 0.7.0` (tag `v0.7.0` para anvil/smallvil), com `wayland-server 0.31.x`,
   `wayland-protocols-wlr 0.3.8`, `calloop 0.14.x`. Toolchain 1.100.0.
2. `webkit6 = 0.6.1` com features `v2_52`/`gtk_v4_18` contra `libwebkitgtk-6.0-dev 2.52.6`.
3. `gtk4-layer-shell = 0.8.0` do crates.io, não `librust-gtk4-layer-shell-dev` do apt (defasado);
   biblioteca C do sistema é `libgtk4-layer-shell-dev 1.3.0`.
4. `ts-rs = 12.0.1` para gerar tipos Rust→TS. `specta` estável é 1.0.5 e 2.0 ainda é RC.
5. **Não** usar a crate `greetd_ipc` (GPL-3.0-only): implementar o IPC localmente a partir da
   especificação `greetd-ipc(7)`. A crate pode ser usada apenas como oráculo de teste em dev.
6. Chamada real ao greetd: `post_auth_message_response` (não `request_answer`); framing
   `u32` em byte order nativo seguido de JSON UTF-8.

## Consequências
Menos risco de incompatibilidade de API e nenhuma contaminação de licença. A implementação do
IPC greetd entra no S7 com testes contra mock e, separadamente, contra greetd real em VM.
