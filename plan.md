# AeroShell — plano de trabalho (F0 em execução)

FRAMEWORK para compositores Wayland completos cuja parte visual é escrita em TSX/JSX e renderizada em WebViews nativas (WebKitGTK 6.0). O mecanismo (Rust/Smithay) é dono do estado e do input; a política (Bun/TypeScript) decide layouts, regras e idle. Nada de GTK/WebKit no compositor; nada de Smithay nos clientes.

> Detailhe executivo e histórico em `docs/progress.md`. Este arquivo é o plano sincronizado com ele.

Ambiente validado em 2026-11-18: Ubuntu 26.04.1 (resolute), Rust 1.100.0, Bun 1.4.4, GTK 4.22.4, WebKitGTK 2.52.6, wayland 1.24.0, wayland-protocols 1.47, libxkbcommon 1.13.1. VM VirtualBox sem 3D: **validar funcionamento; desempenho só em GPU real**.

## Versões fixadas

### apt
`libgtk-4-dev` 4.22.4 · `libwebkitgtk-6.0-dev` 2.52.6 · `libgtk4-layer-shell-dev` 1.3.0-1 · `libwayland-dev` 1.24.0 · `wayland-protocols` 1.47 · `libxkbcommon-dev` 1.13.1 · `libdrm-dev` 2.4.131 · `libinput-dev` 1.31.1 · `libudev-dev` 259.9. A instalar quando necessário: `libgbm-dev`, `libseat-dev`, `libpam0g-dev`, `dbus`/`dbus-run-session`, `greetd`, `swaylock`.
**Não usar**: `librust-gtk4-layer-shell-dev` (binding defasado), `wpewebkit`, `libwpe`, `wlr-protocols`.

### crates.io (pin exato no workspace)
- `smithay = "=0.7.0"` (tag `v0.7.0`; MSRV 1.80.1). Resolve `wayland-server 0.31.14`, `wayland-backend 0.3.17`, `wayland-protocols 0.32.13`, `calloop 0.14.x`.
- `calloop = "=0.14.5"`, `rmp-serde = "=1.3.1"`, `serde = 1`, `serde_json = 1`, `zeroize = 1`.
- `gtk4 = "=0.11"`, `webkit6 = "=0.6.1"` (features `v2_52`, `gtk_v4_18`), `gtk4-layer-shell = "=0.8.0"` (crates.io).
- `wayland-scanner 0.31.11`, `wayland-client 0.31.15`, `bitflags 2.13.2` (protocolo privado gerado).
- `pam = "=0.8.0"` (avaliação de FFI auditável pendente).
- `ts-rs = "=12.0.1"` (specta estável é 1.0.5; 2.0 é RC).
- `greetd_ipc` **não é dependência**: GPL-3.0-only; implementar IPC localmente.

### Ferramentas
`rust-toolchain.toml` fixado em 1.100.0. Bun 1.4.4. `just` disponível; comandos também via `cargo`/`bun`.

### Ambiente de build (validado sem env customizado)
`pkg-config --modversion gtk4-layer-shell-0` → `1.3.0` (Ubuntu 26.04). Combinação validada:
lib C 1.3.0-1 + crate Rust 0.8.0 (mínimo declarado 1.0). `cargo check`, `cargo test` e
`cargo clippy -- -D warnings` passam sem `PKG_CONFIG_PATH`/`LD_LIBRARY_PATH`/`RUSTFLAGS`.
Debian trixie 1.0.x ainda não validado. `scripts/doctor.sh` (chamado por `just check`) nomeia
o pacote apt faltante antes do build.

### Correções de spec apuradas
- greetd: campo `post_auth_message_response`; framing `u32` em **byte order nativo** + JSON UTF-8; `auth_message_type ∈ {visible, secret, info, error}`; `error_type ∈ {auth_error, error}`.
- gtk4-layer-shell: `LD_PRELOAD` mantido como defesa; falha silenciosa **não reproduziu** na lib C 1.3.0, falta testar 1.0.x (trixie). Ver `docs/nested-dev.md`.
- `--size`: winit 0.30 não expõe `Window::set_size`; janela host fica 1280x800 enquanto o modo lógico do output é o pedido (ex.: 1000x650). `WinitEvent::Resized` ignorado quando `--size` é forçado.

## F0 — spikes go/no-go

| Spike | Estado | Evidência |
|---|---|---|
| S1 — compositor nested + socket próprio | **concluído** (`7d816cf`) | `wayland-aero-0`, globals xdg/layer, cliente real dentro da janela, host intacto |
| S2 — painel GTK4/WebKitGTK + layer-shell | **concluído** (`ad26564`, `105d1cb`) | `Anchor(BOTTOM|LEFT|RIGHT)`, `exclusive_zone=48`, transparência, `aero://shell-reset.css`, scrollbar eliminada |
| S3 — input regions derivadas do DOM | **concluído** (`8b97ad8`) | `data-aero-interactive` + observers, cursor mão/padrão, passagem de clique validada manualmente. **Sem latência medida** |
| S4 — ponte messageHandlers ⇄ Rust ⇄ Bun | **não iniciado** | handlers registrados no shell; IPC MessagePack/ts-rs pendente |
| S5 — `aero_frame_v1` | **protocolo experimental congelado** (`09153e3`, `2a781ba`, `cfa9867`, `52f9731`) | XML v1, código gerado, segurança por socketpair, 5 testes de protocolo; **desenho do frame ainda não implementado** |
| S6 — ext-session-lock + fail-secure | não iniciado | — |
| S7 — PAM + greetd mock | não iniciado | — |

### Decisões registradas
- **ADR 0001 (aceito provisoriamente)**: `related-view` padrão com âncora oculta viva; pool 2–4 frames pré-aquecidos para **latência**, não memória; atlas só como Plano B se em GPU real PSS adicional com 10 janelas > 150 MB e/ou p95 de resize > 16,7 ms; dados de VM sem 3D, `WebKitGPUProcess=0`.
- **ADR 0002**: versões e licenças; `greetd_ipc` GPL-3.0 fora.
- **ADR 0003 (corrigido)**: apenas `ext-background-effect-v1`; Smithay git já tem handler, `wayland-protocols 0.32.13` tem bindings; custo real é o render (médio, prior art no niri). Trocar para rev fixa do Smithay git avaliada no fim do F0/F1.

### Benchmark S5 sem protocolo (release, VM sem 3D, 3 repetições, estabilização <2%)
- Sem `related-view`, N=20 → 20 WebKitWebProcess, PSS ~1009–1011 MB.
- Com `related-view`, N=20 → **1 WebKitWebProcess**, PSS ~500–505 MB.
- Atlas 1 WebView/20 componentes → 1 WebProcess, PSS ~304–310 MB; DOM 0,2 ms / 0,01 ms; buffer 2.452.000 bytes.
- Baseline `aeroshell-shell`: PSS 109744 kB (~73 MB libs compartilhadas, ~36 MB heap/anon; libLLVM 27583 kB).

### Protocolo `aero_frame_v1` (experimental, congelado para F0)
- Segurança: `AERO_PRIVILEGED_FD` → `OwnedFd` → `UnixStream` → `insert_client` com `ClientState{is_shell:true}`; `can_view` só olha essa marca. Cliente do socket público nunca vê o global, mesmo com `AERO_SHELL_PID` forjado.
- `role` via `give_role`; `already_constructed` e `serial` inválido protegem a fronteira; `unknown_window` gera `closed` (corrida legítima, não erro fatal).
- Testes reais: `tests/integration/aero_frame_protocol.sh` → (a) global público invisível, (a2) roundtrip privilegiado, (b) already_constructed, (c) role, (d) serial falso ignorado.

### Métrica do protótipo do frame (congelada antes de medir)
Tempo, em ms, entre `frame.configure(serial, w, h)` e o `wl_surface.commit` do frame com o novo tamanho, **por passo de resize**. Estatística: média, p95 e frames perdidos (sem commit até timeout de `ack_configure` = 100 ms). Execução: release, 3 repetições, N=5 e N=20, VM sem 3D (relativo); veredito final em GPU real.

## Próximo passo
Protótipo mínimo do frame (retângulo + titlebar em HTML, sem estilização): posicionar a superfície do frame atrás do conteúdo, `configure`/`ack_configure` com timeout, drag e resize em Rust; medir com a métrica acima. **Parar e reportar antes da Fase 1.**

## Fases seguintes (uma por vez, só após aprovação)
F1 núcleo do compositor (trait Backend winit/headless/udev, damage, protocolos, supervisor) · F2 shell host + SDK v0 + `aero dev` · F3 WM · F4 serviços · F5 idle + lock · F6 greeter · F7 compatibilidade · F8 DX/packaging · F9 endurecimento.

## Regras de fronteira
- `aeroshell-compositor` não conhece GTK/WebKit; `shell`/`lock`/`greeter` não conhecem Smithay.
- Só `backend/*.rs` conhece nested vs real vs headless.
- Tempo-real e segredos em Rust; política em Bun. Pool reduz latência, `related-view` reduz memória.
- Supervisor: socketpair privado, nunca `WAYLAND_SOCKET` no ambiente do compositor.
- Nunca alterar o host: `WAYLAND_DISPLAY` só no env dos filhos, D-Bus privado em nested, sem `/etc/pam.d` sem aviso.

## Commits relevantes
`7d816cf` S1 · `ad26564`/`105d1cb` S2 · `8b97ad8` S3 · `d7e8993` ADR/benchmark · `09153e3` protocolo gerado · `cfa9867` socketpair + testes · `52f9731` ADR 0003 corrigido.
