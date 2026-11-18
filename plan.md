# AeroShell — plano de trabalho (F0 em execução)

FRAMEWORK para compositores Wayland completos cuja parte visual é escrita em TSX/JSX e renderizada em WebViews nativas (WebKitGTK 6.0). O mecanismo (Rust/Smithay) é dono do estado e do input; a política (Bun/TypeScript) decide layouts, regras e idle. Nada de GTK/WebKit no compositor; nada de Smithay nos clientes.

Ambiente validado em 2026-11-18: Ubuntu 26.04.1 (resolute), Rust 1.100.0, Bun 1.4.4, GTK 4.22.4, WebKitGTK 2.52.6, wayland 1.24.0, wayland-protocols 1.47, libxkbcommon 1.13.1. VM VirtualBox sem 3D: **validar funcionamento; desempenho só em GPU real**.

## Versões fixadas

### apt
`libgtk-4-dev` 4.22.4 · `libwebkitgtk-6.0-dev` 2.52.6 · `libgtk4-layer-shell-dev` 1.3.0-1 (instalar no S2) · `libwayland-dev` 1.24.0 · `wayland-protocols` 1.47 · `libxkbcommon-dev` 1.13.1 · `libdrm-dev` 2.4.131 · `libinput-dev` 1.31.1 · `libudev-dev` 259.9. A instalar quando necessário: `libgbm-dev`, `libseat-dev`, `libpam0g-dev`, `dbus`/`dbus-run-session`, `greetd`, `swaylock`.
**Não usar**: `librust-gtk4-layer-shell-dev` (binding defasado), `wpewebkit`, `libwpe`, `wlr-protocols`.

### crates.io (pin exato)
- `smithay = "=0.7.0"` (tag `v0.7.0`; MSRV 1.80.1). Resolve `wayland-server 0.31.x`, `wayland-protocols-wlr 0.3.8`, `wayland-protocols-misc 0.3.8`, `calloop 0.14.x`.
- `calloop = "=0.14.5"`, `rmp-serde = "=1.3.1"`, `serde = 1`, `serde_json = 1`, `zeroize = 1`.
- `gtk4 = "=0.11"`, `webkit6 = "=0.6.1"` (features `v2_52`, `gtk_v4_18`), `gtk4-layer-shell = "=0.8.0"` (crates.io).
- `pam = "=0.8.0"` (únicos estável; avaliar FFI auditável em ADR).
- `ts-rs = "=12.0.1"` (specta estável é 1.0.5; 2.0 é RC).
- `greetd_ipc` **não é dependência**: é GPL-3.0-only; implementar o IPC (~200 LOC) a partir da spec.

### Ferramentas
`rust-toolchain.toml` fixado em 1.100.0 (instalado). Bun 1.4.4. `just` a instalar (ou usar `cargo`/`bun` diretamente).

### Correções de spec já apuradas
- greetd: campo é `post_auth_message_response`; framing `u32` em **byte order nativo** + JSON UTF-8; `auth_message_type ∈ {visible, secret, info, error}`; `error_type ∈ {auth_error, error}`.
- gtk4-layer-shell: carregar **antes** de libwayland-client (ordem de link / `LD_PRELOAD=`), senão a janela vira toplevel comum sem erro claro. Documentar em `docs/nested-dev.md`.
- `ListeningSocketSource::with_name("wayland-aero-0")` é a API real do Smithay 0.7 para socket próprio.

## F0 — spikes go/no-go (fase atual)

Cada spike termina com `cargo check`, teste automatizado e entrada em `docs/progress.md`.

| Spike | Objetivo | Aceite |
|---|---|---|
| S1 | Compositor Smithay nested (winit), socket próprio `wayland-aero-0`, xdg-shell + layer-shell, cliente aberto dentro da janela | `WAYLAND_DISPLAY` do host intacto; cliente conecta só no socket do AeroShell; resize da janela muda modo do output; `--outputs 2` |
| S2 | Painel GTK4 + WebKitGTK 6.0 via gtk4-layer-shell, fundo transparente, `exclusive_zone` | Alpha != 255 no framebuffer; zona exclusiva reduz área útil; ordem de link documentada |
| S3 | Passagem de input: `data-aero-interactive` → `wl_surface.set_input_region` | Cliques dentro/fora com precisão ≤2 px; cursor correto; fallback bounding box |
| S4 | `messageHandlers` ⇄ Rust ⇄ Bun, MessagePack, snapshot/patch com revisão | Round trip p99 < 1 ms (medido, com ressalva de VM); tipos TS gerados por ts-rs |
| S5 | `aero_frame_v1`, N=5 e N=20, pool de WebViews | RAM/processos/custo/latência em GPU real. **Se falhar: ADR com alternativas e PARAR para decisão do usuário** |
| S6 | `ext-session-lock-v1`, cliente mínimo + swaylock, fail-secure | Matar cliente mantém trancado, respawn, sem input, screencopy recusado |
| S7 | PAM fora da thread principal + greetd mock | Prompts genéricos, zeroize, senha fora de logs/Bun; handshake create→auth→start_session |

Entrega: `docs/adr/0001-go-no-go.md`, `docs/adr/0002-pins-e-licencas.md`, `docs/progress.md`, dados em `tests/bench/results/`.

## Fases seguintes (uma por vez, só após aprovação)
F1 núcleo do compositor (trait Backend winit/headless/udev, damage, protocolos, supervisor) · F2 shell host + SDK v0 + `aero dev` · F3 WM · F4 serviços · F5 idle + lock · F6 greeter · F7 compatibilidade · F8 DX/packaging · F9 endurecimento.

## Regras de fronteira
- `aeroshell-compositor` não conhece GTK/WebKit; `shell`/`lock`/`greeter` não conhecem Smithay.
- Só `backend/*.rs` conhece nested vs real vs headless.
- Tudo que é tempo-real e sensível (lock, auth, senha) em Rust; política em Bun.
- Comandos e patches atômicos; compositor é a fonte da verdade; sobrevivência a crash do shell/runtime.
- Nunca alterar o ambiente do host: `WAYLAND_DISPLAY` só no env dos filhos, D-Bus privado em nested, sem `/etc/pam.d` sem aviso.

## Estado atual
- [x] Ambiente verificado e versões fixadas.
- [x] Esqueleto do workspace (Cargo.toml, package.json, justfile, rust-toolchain.toml, diretórios).
- [ ] S1 em implementação.
