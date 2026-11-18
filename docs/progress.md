# Progresso

## F0 — spikes go/no-go

### Concluído
- Verificação de ambiente e versões (2026-11-18): Ubuntu 26.04.1, Rust 1.100.0, Bun 1.4.4,
  GTK 4.22.4, WebKitGTK 2.52.6, wayland 1.24.0, wayland-protocols 1.47.
- Correção de spec: greetd usa `post_auth_message_response`, framing `u32` nativo + JSON.
- `greetd_ipc` é GPL-3.0-only → implementar protocolo localmente.
- Binding Rust do gtk4-layer-shell no apt está defasado → usar crates.io 0.8.0 + lib C 1.3.0.
- Esqueleto do workspace: `Cargo.toml`, `package.json`, `justfile`, `rust-toolchain.toml`,
  8 crates, diretórios F0, `.gitignore`. `cargo check --workspace --all-targets` limpo.

### S1 — concluído (2026-11-18)
- [x] Trait `Backend` + autodetecção (`WAYLAND_DISPLAY`/`DISPLAY` → winit; senão udev).
- [x] Socket próprio via `ListeningSocketSource::with_name("wayland-aero-0")`.
- [x] Globals: `wl_compositor`, `wl_shm`, `xdg_wm_base`, `zwlr_layer_shell_v1`, `wl_output`, seat,
      data-device e xdg-output.
- [x] Renderização GLES + damage; `WinitEvent::Resized` altera o modo do output.
- [x] `--outputs 2`, `--scale`, `--size`, `--mod`, `--command` (filho com env isolado via `sh -c`).
- [x] Teste de integração `tests/integration/s1_nested.sh`: socket criado, `WAYLAND_DISPLAY` do host intacto.
- [x] Validação real: `--backend winit --outputs 2 --command "zenity --info --text=AeroShell-S1"`
      conectou cliente ao socket do AeroShell; EGL/GLES inicializou (VMware SVGA3D/LLVM — VM sem 3D);
      nenhum erro de renderização.

**Riscos observados**: `--size` define o modo do output, não redimensiona a janela host (a API
winit do Smithay 0.7 não expõe `set_size` no backend; `Resized` continua funcionando na direção
janela → output). Multi-output em F0 cria globals e modos, mas renderiza apenas o primeiro output;
multi-output real entra em F1.

### Pendente
S2 (requer `sudo apt-get install libgtk4-layer-shell-dev`), S3, S4, S5, S6, S7 e ADR go/no-go.
