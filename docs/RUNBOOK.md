# RUNBOOK.md — como compilar, rodar e testar
 
Se um comando abaixo estiver desatualizado, CORRIJA este arquivo na mesma tarefa em que você descobriu.
 
## Pré-requisitos
- Rust via rustup (versão em `rust-toolchain.toml`), Bun, `libwebkitgtk-6.0-dev`, `libgtk-4-dev`, `libgtk4-layer-shell-dev`, `foot`.
- Backend de sessão real (obrigatório desde F1d1, feature padrão do `aeroshell-compositor`):
  `libdrm-dev libgbm-dev libinput-dev libudev-dev libseat-dev libsystemd-dev`
  (fallback `libelogind-dev` se `libseat-dev` não existir na imagem).
- Instala tudo: `bash scripts/setup-deps.sh`. Verifica: `bash scripts/doctor.sh` (o gate chama o doctor).
- Build sem o backend de sessão real (nested/headless apenas): `cargo check --workspace --no-default-features`.
- Rode `bash scripts/check.sh` primeiro; ele chama o `doctor` e diz o que falta.
## Gate de qualidade (obrigatório antes de dizer "concluído")
```
bash scripts/check.sh      # doctor + fmt --check + clippy -D warnings + cargo test --workspace + bun test
```
Gate verde = TUDO verde numa única execução. Se `bun test` ou o S1 falhar, NÃO reexecute até passar: registre em
`docs/STATE.md` §6 e abra a tarefa `00-gate-confiavel.md`. O S1 já falhou uma vez sob carga (exit 1, ~16 s) e passou
isolado (7,67 s); isso está registrado, não descartado.

## Validação visual (juiz humano)
Para qualquer critério visual — arrastar/fechar janela, largura do painel, cursor, transparência, z-order — execute o
comando abaixo, **deixe a aplicação aberta**, diga exatamente o que observar e peça o "ok" do humano. Quem fecha a
aplicação é o humano, nunca o agente (AGENTS.md §0.9). Sem o "ok", o item fica em `STATE.md` §7 como não validado.

## Rodar o desktop nested (janela dentro do seu desktop)
```
cargo build --workspace --examples
LD_PRELOAD=libgtk4-layer-shell.so.0 target/debug/aeroshell-compositor \
  --backend winit --socket wayland-aero-demo \
  --shell target/debug/examples/frame_webview \
  --shell-env LD_PRELOAD=libgtk4-layer-shell.so.0 \
  --command foot
```
Teste manual: arraste a barra (move), as bordas de 6 px (resize), botões max/✕.
Para encerrar: feche a janela do compositor. Confirme com
`ps -eo pid,etime,args | grep -E "aeroshell|frame_webview|WebKit" | grep -v grep` que nada sobrou.

### Validação visual F1c (dois outputs, dois `foot`)
```
cargo build --workspace --examples
dbus-run-session -- target/debug/aeroshell-compositor \
  --backend winit --socket wayland-aero-demo2 --size 640x480 --outputs 2 \
  --shell target/debug/examples/frame_webview \
  --shell-env LD_PRELOAD=libgtk4-layer-shell.so.0 \
  --command "foot -w 400x300"
```
Em outro terminal, com o mesmo D-Bus privado: `WAYLAND_DISPLAY=wayland-aero-demo2 foot -w 320x240`.
Arraste a titlebar do `foot` da direita para o segundo tile e de volta: o ponteiro cobre a janela host
inteira (1280×480), cada output desenha só o seu cliente e nenhum frame aparece ocupando a metade da tela.
 
## Rodar em DRM (udev) — sessão real no VT
**Status medido em F1d1:** o backend udev é **stub** — `crates/aeroshell-compositor/src/backend/udev.rs:20-26`
devolve `Err("backend udev: implementação na F1 (requer libgbm/libseat e sessão real)")`. Desde F1d1 os crates
do backend de sessão real estão no grafo e no binário
(`cargo tree -p aeroshell-compositor | grep -cE "(gbm|drm|drm-ffi|input|udev|libseat) v"` → 6;
`nm -a --defined-only target/debug/aeroshell-compositor | grep -E " a (gbm|libseat|udev|input|drm)\."` → CGUs
dos 5 crates), **mas `ldd` não lista as `.so`** (medido: `DT_NEEDED` só tem libxkbcommon/libgcc/libc), porque
nenhum código as chama ainda. Nada aparece na tela com `--backend udev`; a implementação chega em **F1d3**.
Em F1d1 este comando termina com o erro do stub — registre a saída crua, não a interprete como regressão.

Comando exato a partir do **tty3** (executado, olhado e **fechado pelo humano**):
```
# tty3, logado como o usuário da sessão (líder de sessão do logind; sem sudo no host de dev)
cd ~/Projetos/aeroshell
cargo build --workspace
XDG_RUNTIME_DIR=/run/user/$(id -u) MESA_DEBUG=1 LIBGL_ALWAYS_SOFTWARE=1 \
  target/debug/aeroshell-compositor --backend udev --socket wayland-aero-tty3 \
  --shell target/debug/examples/frame_webview \
  --shell-env LD_PRELOAD=libgtk4-layer-shell.so.0 \
  --command foot
```
- `--backend udev` é aceito pela CLI (`src/cli.rs:85`); sem compositor host (`WAYLAND_DISPLAY`/`DISPLAY`
  ausentes) a autodetecção já escolhe udev (`src/cli.rs:63-70`).
- **Trocar o VT (`Ctrl+Alt+F3`) e fechar a sessão é tarefa do humano** (AGENTS.md §0.9). O agente não troca
  VT, não usa `sudo`, não toca em `/etc` (AGENTS.md §3.8).
- **Validação visual exigida em F1d3** (juiz humano, não teste headless): "no tty3 aparecem o painel e a
  janela `foot`; arrastar a barra move só a janela dona; não há cursor fantasma; a tela não pisca no resize"
  → o humano dá o ok e fecha.
- VM VirtualBox **sem 3D**: `LIBGL_ALWAYS_SOFTWARE=1` é o caminho em dev; tempo/RAM medidos aqui **não**
  valem como desempenho (GPU real pendente, STATE.md §7).

## Runtime Bun / SDK (tarefa 03)
Tipos TS gerados do Rust (não editar à mão):
```
bun run generate:types   # cargo test -p aeroshell-ipc --features export-bindings
```
Com o nested acima aberto e 2 `foot`, em outro terminal do mesmo diretório:
```
export AERO_WINDOW_IPC_SOCK=<o caminho passado ao compositor>
bun run packages/runtime/src/main.ts --list          # imprime "<id>\t<título>" por janela
bun run packages/runtime/src/main.ts --focus 0       # eleva a janela 0
bun run packages/runtime/src/main.ts --close 1       # fecha só a janela 1
```
O socket IPC só existe se `AERO_WINDOW_IPC_SOCK` estiver definido ao iniciar o compositor.

## Testes de integração (precisam de display; rodar à mão, não no CI)
```
python3 tests/integration/frame_resize_edges.py   # 8 arestas/cantos
python3 tests/integration/frame_buttons.py        # max/restaurar/fechar
python3 tests/integration/frame_fixed_canvas_release_pixels.py         # 02p: release full-res
python3 tests/integration/frame_fixed_canvas_release_pixels.py growth  # 02p: target > canvas
python3 tests/integration/frame_fixed_canvas_hygiene.py                # 02p: sem ReleaseSwap
python3 tests/integration/frame_repeated_activate.py                   # 02p: activate idempotente
python3 tests/integration/frame_cold_start.py     # 02q: placeholder/pool/related-view + cronômetro map→1º render
python3 tests/integration/headless_render_damage.py # F1b/c: age>0, damage parcial e pixels do 9-slice
python3 tests/integration/multi_output_damage.py  # F1c: 2 outputs, damage incremental e conteúdo nos 2 tiles
python3 tests/integration/frame_window_id.py      # 2 janelas, cada frame com o próprio window_id
python3 tests/integration/sdk_cli.py              # 03: --list/--focus/--close do runtime Bun
python3 tests/integration/supervisor_lifecycle.py # kill -9 do compositor derruba os filhos
python3 tests/integration/supervisor_real.py      # fluxo real shell privilegiado
python3 tests/bench/run_frame_bench.py            # benchmark de protocolo (VM ruidosa: sem conclusão)
```
Testes de protocolo headless (CI): `cargo test -p aeroshell-compositor`.
 
## Armadilhas conhecidas
- `pgrep/pkill -x` com nome > 15 caracteres NUNCA casa (o kernel trunca `comm`). Use `-f` com padrão específico.
- gtk4-layer-shell: manter `LD_PRELOAD` (não reproduziu o problema de ordem na 1.3.0; falta testar a 1.0.x do trixie).
- O host (GNOME/Mutter) NÃO tem layer-shell: os clientes do shell SEMPRE conectam ao socket do AeroShell.
- VM sem 3D: números de tempo/RAM são só relativos. Se `glxinfo -B` mostrar llvmpipe, é software.
- winit 0.30 não expõe `Window::set_size`: janela host 1280x800, output lógico pode ser outro (`--size`).
- Em sessão X11 o nested abre via X11; confirme também numa sessão Wayland antes de concluir sobre input.
- Variáveis `AERO_*` de benchmark só podem existir sob feature `bench`; o binário release não deve lê-las.
## Onde mexer para…
- Backend de sessão real (udev/DRM/GBM/libseat): `crates/aeroshell-compositor/src/backend/udev.rs` (stub em F1d1; implementação em F1d3)
- Features de build do backend: `crates/aeroshell-compositor/Cargo.toml` → `[features]` (`default = [backend_udev, backend_gbm, backend_libinput, backend_session_libseat]`)
- Protocolo Wayland do frame: `protocols/aero_frame_v1.xml` (PROTEGIDO) + `crates/aeroshell-compositor/src/protocols/aero_frame.rs`
- Grabs de mover/resize: `crates/aeroshell-compositor/src/grab.rs`
- Supervisor/filhos: `crates/aeroshell-compositor/src/supervisor.rs`
- Frame WebView: `crates/aeroshell-shell/examples/frame_webview.rs`
- CSS base de todas as superfícies: `assets/shell-reset.css` (servido por `aero://`)
- IPC e tipos: `crates/aeroshell-ipc`
 