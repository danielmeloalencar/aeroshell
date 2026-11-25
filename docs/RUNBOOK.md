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
# Sem AERO_WINDOW_IPC_SOCK o JS dos botões max/✕ envia o comando, mas o shell o
# descarta ("AERO_WINDOW_IPC_SOCK ausente; comando ... ignorado") — os botões
# parecem mortos mesmo com o clique chegando à WebView.
export AERO_WINDOW_IPC_SOCK=/tmp/wayland-aero-demo-ipc-$$
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
 
## Sonda GBM fora do VT (F1d2)
Diagnóstico sem trocar VT, sem master, sem sessão libseat e sem socket Wayland — pode rodar dentro do host:
```
cargo build --workspace
python3 tests/integration/udev_drm_probe.py
```
Ou direto: `target/debug/aeroshell-compositor --probe-gbm=/dev/dri/renderD128` (valor padrão se omitido).
Imprime JSON com `backend_name`, formats RENDERING|SCANOUT, modifiers, uma alocação `gbm_bo_create` e outra via
`GbmAllocator` (ambas exportadas como dmabuf), mais conectores/modos de `/dev/dri/card0` (variável
`AERO_DRM_CARD_NODE` sobrescreve o caminho). O `--no-default-features` recusa a sonda com mensagem explícita.
Requer acesso de leitura/escrita aos nós `/dev/dri/*` (no ambiente de dev há ACL `user:daniel:rw-`).

## Rodar em DRM (udev) — sessão real no VT
**Status em F1d3:** o backend udev está **implementado** (`backend/udev.rs::init_udev`): libseat + udev +
DRM/GBM + EGL/GLES + libinput + `DrmOutputManager`, com a cena compartilhada de `render/scene.rs`.
F1d2 já havia provado `libgbm.so.1` em `DT_NEEDED`. Nesta VM (sem VT ativo) a inicialização chega até
`sessão libseat aberta` e `dispositivo DRM selecionado = /dev/dri/card0`, e falha em `session.open()` com
`EAGAIN` — **esperado**: sem VT ativo a seat não concede o device. Isso NÃO é regressão; é o limite do
ambiente. A renderização real só pode ser julgada no tty3, pelo humano.

Script versionado: **`scripts/run-udev-diag.sh`**. Para quem não consegue copiar/colar no tty, existe o
atalho **`~/u`** (uma linha, fora do repo): no tty3 basta digitar `~/u` e Enter. Argumentos são repassados
ao compositor — ex.: `~/u --scale 1.5` (F1f). O `~/run-aeroshell-udev.sh`
antigo agora só repassa para o script versionado. O script faz build incremental, pré-checagens (binários,
`/dev/dri/*`, `XDG_RUNTIME_DIR`), limpa `DISPLAY`/`WAYLAND_DISPLAY`, fixa `RUST_LOG=debug` e
`LIBGL_ALWAYS_SOFTWARE=1`, grava `~/aeroshell-udev-YYYYMMDD-HHMMSS.log` via `tee` e imprime o que observar.
`Ctrl+C` encerra. Testado nesta VM sem VT: `EXIT=1` em `session.open()` com `EAGAIN`, e o log contém
`UDEV_SESSION active=false seat=seat0` — prova de que o `RUST_LOG=debug` chegou ao instrumento.

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
- **Trocar o VT e fechar a sessão é tarefa do humano** (AGENTS.md §0.9). O agente não troca VT, não usa
  `sudo`, não toca em `/etc` (AGENTS.md §3.8). Como o libinput "graba" os `/dev/input/event*`, o atalho do
  kernel (`Ctrl+Alt+F*n*`) não chega ao VT: o compositor intercepta `Ctrl+Alt+F1..F4` e chama
  `session.change_vt(n)` (`backend/udev.rs`, diagnóstico F1d3). Use `Ctrl+Alt+F2` para sair e `Ctrl+Alt+F3`
  para voltar; se o compositor morrer, o kernel volta a aceitar o atalho.
- **Validação visual PENDENTE (juiz humano, não teste headless)**: no tty3, confirmar que aparecem o painel e
  a janela `foot`, que arrastar a barra move só a janela dona, que a tela não pisca no resize e que trocar
  para outro VT e voltar retoma a imagem. **Cursor ainda NÃO é renderizado no backend udev** (Smithay 0.7.0
  não tem `PointerRenderElement`/`SoftwareCursor`; STATE.md §6) — o ponteiro move e clica, mas é invisível.
  O humano dá o ok e fecha; o agente não encerra o processo.
- VM VirtualBox **sem 3D**: `LIBGL_ALWAYS_SOFTWARE=1` é o caminho em dev; tempo/RAM medidos aqui **não**
  valem como desempenho (GPU real pendente, STATE.md §7).

### Diagnóstico da tela sem imagem (cartão `F1d3-diagnostico-visual.md`)
Se o tty3 ligar mas não mostrar imagem, digitar **apenas `~/u`** no tty (o script já fixa `RUST_LOG=debug`
e grava o log). Equivalente, se quiser ver o comando:
```
cd ~/Projetos/aeroshell
./scripts/run-udev-diag.sh
```
O script compila o compositor com **`--features bench`** (desde 2026-11-22): é assim que existem as
sondas `FOOT_COMMIT` (commit do cliente + callbacks pendentes) e `UDEV_FRAME_CB` (callbacks pendentes
no loop udev). Sem `bench` o log fica sem essas linhas — foi o que aconteceu em
`/home/daniel/aeroshell-udev-20261122-051134.log`. Desde 2026-11-22 o script também compila e lança
**`aeroshell-shell`** (o painel React/TSX, layer surface `aeroshell-panel`) como cliente público via
`WAYLAND_DISPLAY=wayland-aero-tty3`; o `frame_webview` continua sendo o decorador privilegiado
(`--shell`). O painel é um PID iniciado pelo script e é encerrado por ele quando o compositor sai.
O script também define `AERO_WINDOW_IPC_SOCK` para o compositor **e** o painel: sem essa variável o
compositor não cria o IPC de janelas e o painel fica com `lista=` vazia (barra transparente, invisível).

No log, procurar `UDEV_SESSION`, `UDEV_OUTPUT`, `UDEV_FRAME`, `UDEV_VBLANK`, `UDEV_SESSION_EVENT`,
`UDEV_VT`, `CLIENT_ELEMENT_PROBE`, `FOOT_COMMIT` e `UDEV_FRAME_CB`. O analisador roda em qualquer máquina com o arquivo de log:
```
python3 tests/integration/udev_frame_probe.py ~/aeroshell-udev-YYYYMMDD-HHMMSS.log
```
`VEREDITO: OK` exige sessão ativa, CRTC ativo, `UDEV_FRAME ... queued=ok` e `UDEV_VBLANK ... submitted=ok`.
Ele **não** mede pixels — a imagem na tela continua sendo julgada pelo humano. Para medir se o cliente
está composto, rode também:
```
python3 tests/integration/udev_client_compose_probe.py ~/aeroshell-udev-YYYYMMDD-HHMMSS.log
```
(distingue `CALLBACK_NAO_CHEGA` de `SEM_DAMAGE_FORA_DA_CENA`; exige ~10 s de interação com o `foot`.)

## Runtime Bun / SDK (tarefa 03)

### `desktop.config.tsx` e superfícies sem framework
```
bun run generate:types
bun build packages/sdk/src/client.ts --target=browser --outfile=packages/sdk/dist/client.js
cargo build --workspace --examples
cargo build -p aeroshell-shell   # --examples NÃO relinka este binário
cargo build -p aeroshell-compositor --features bench  # por último nos harnesses de sonda
python3 tests/integration/f2_sdk_surfaces.py
```
O harness sobe compositor/headless, host genérico e `examples/blank-desktop/desktop.config.tsx` por
`bun packages/runtime/src/main.ts --config <arquivo>`. Mede background/painel, z-order, update
input/blur da própria surface e clique pass-through. Em nested, inicie também o host genérico com
`AERO_SURFACE_HOST=1 WAYLAND_DISPLAY=<socket> target/debug/aeroshell-shell`; frames continuam em
`--shell target/debug/examples/frame_webview`. Blur é somente declaração nesta fase.

### HMR do desktop (`F2-HMR`, somente desenvolvimento)
Defina `AERO_DEV_ORIGIN=http://127.0.0.1:4173` no compositor, no host genérico e no runtime. Só o
runtime recebe a flag; o host/compositor usam a origem como allowlist de navegação:
```
bun packages/runtime/src/main.ts --config examples/blank-desktop/desktop.config.tsx --dev
```
`desktop.config.tsx` é empacotado novamente a cada edição (inclusive helpers) e gera diff
create/update/destroy. `dev_entry: "./panel.ts"` marca o módulo browser construído por `Bun.build`;
edições dele disparam `enableHmr()` via WebSocket same-origin. Sem `--dev`, o servidor não existe,
HTTP é recusado e o HTML inline `aero://` permanece como fallback. O servidor não aceita conexões
fora de `127.0.0.1`, valida `Origin` do WebSocket e não deve ser usado em produção.

Harness headless completo (edita cor/texto, módulo e create/destroy; mede pixels e WebProcesses):
```
python3 tests/integration/f2_hmr.py
```

### Outputs e atalhos globais (`F2-outputs+input-binds`)
`aero.outputs.list/get/subscribe` expõe nome, geometria lógica, escala e modo físico; o stream faz
replay com `OutputAdded`, resize winit publica `OutputChanged` e o contrato inclui `OutputRemoved`.
Hotplug/remoção dinâmica udev NÃO está implementada. `SurfaceSpec.output` escolhe o monitor GTK e o
`LayerMap`; IDs fora da lista são recusados.
`aero.input.bind/unbind/list` existe apenas no runtime privilegiado. O guest continua restrito a
input/blur da própria surface. No config, a ação disponível é `workspace-activate <id>`:
```
keybindings: [
  { keys: "Super+1", command: "workspace-activate 1" },
  { keys: "Super+2", command: "workspace-activate 2" },
]
```
O compositor intercepta press/repeat/release pelo keycode e publica `KeyBind { id, keys }`; callbacks
e ações ficam no TypeScript, nunca num shell. Para testes, `AERO_TEST_INPUT_SOCK` aceita
`key press|release <evdev>` (Super esquerdo=125, tecla 1=2, tecla 2=3).
```
cargo build -p aeroshell-compositor --features bench
python3 tests/integration/f2_outputs_input_binds.py
```
O harness cobre headless + winit de 2 outputs, `LayerMap`, clique no segundo tile, SDK real,
callback/unbind, repeat/release reverso, reload sem duplicar e recusa do guest. O exemplo
`blank-desktop` tem painéis nos outputs 0/1; crie o workspace 2 antes de testar `Super+2`.
O background azul existe só no output 0; o clear escuro no output 1 é esperado. No nested, os dois
outputs são tiles lógicos de uma única janela host e o ponteiro atravessa os lados livremente. O GNOME
pode capturar Super físico; o juiz pode aceitar injeção documentada pelo socket, sem alterar o host.

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

### Notificações (F2-notifications)
Com o nested acima aberto, dispare um toast pelo IPC ou pelo SDK tipado:
```
IPC=${AERO_WINDOW_IPC_SOCK:?defina AERO_WINDOW_IPC_SOCK}
python3 -c 'import os,socket; s=socket.socket(socket.AF_UNIX); s.connect(os.environ["IPC"]); s.sendall(b"notify Teste Corpo da mensagem\n")'
bun -e 'import { createAero } from "./packages/sdk/src/index.ts"; import { ipcTransport } from "./packages/runtime/src/ipc-client.ts"; const aero = createAero(ipcTransport(process.env.IPC)); aero.showNotification("Teste", "Corpo da mensagem", "utilities-terminal"); await new Promise(r => setTimeout(r, 200));'
bun -e 'import { createAero } from "./packages/sdk/src/index.ts"; import { ipcTransport } from "./packages/runtime/src/ipc-client.ts"; const aero = createAero(ipcTransport(process.env.IPC)); aero.showNotification("Longa", "Dez segundos", undefined, 10000); await new Promise(r => setTimeout(r, 200));'
```
O toast aparece no canto inferior direito, acima do painel; some após 5 s (padrão) ou 10 s no segundo exemplo. O “X” fecha na hora. A layer do painel cresce só enquanto há notificação; a área transparente continua passando clique.
O socket IPC só existe se `AERO_WINDOW_IPC_SOCK` estiver definido ao iniciar o compositor.

## Testes de integração (precisam de display; rodar à mão, não no CI)
```
python3 tests/integration/frame_resize_edges.py   # 8 arestas/cantos
python3 tests/integration/frame_buttons.py        # max/restaurar/fechar
python3 tests/integration/frame_fixed_canvas_release_pixels.py         # 02p: release full-res
python3 tests/integration/frame_fixed_canvas_release_pixels.py growth  # 02p: target > canvas
python3 tests/integration/frame_fixed_canvas_hygiene.py                # 02p: sem ReleaseSwap
python3 tests/integration/frame_repeated_activate.py                   # 02p: activate idempotente
python3 tests/integration/popup_client.py         # F1-popups-render: xdg_popup real + grab (exige build --features bench)
python3 tests/integration/f1i_protocols.py        # F1i+: globals text_input/input_method/xdg_activation + ativação real (exige build --features bench)
python3 tests/integration/f1_toplevel_icon.py    # ícone do toplevel até o DOM da titlebar (nested, exige build --features bench)
python3 tests/integration/f1_csd_gtk.py          # CSD: sem barra HTML + move/resize por xdg_toplevel (nested, exige bench)
python3 tests/integration/f1_kde_decoration.py   # global org_kde_kwin_server_decoration (GTK3/gedit)
# Atenção: check.sh reconstrói o bin sem `bench`. Antes dos harnesses de sonda, rode
# cargo build -p aeroshell-compositor --features bench novamente.
python3 tests/integration/frame_cold_start.py     # 02q: placeholder/pool/related-view + cronômetro map→1º render
python3 tests/integration/headless_render_damage.py # F1b/c: age>0, damage parcial e pixels do 9-slice
python3 tests/integration/multi_output_damage.py  # F1c: 2 outputs, damage incremental e conteúdo nos 2 tiles
python3 tests/integration/frame_window_id.py      # 2 janelas, cada frame com o próprio window_id
python3 tests/integration/frame_title.py          # titulo real do toplevel no #title do frame (foot -T, 2 janelas)
python3 tests/integration/sdk_cli.py              # 03: --list/--focus/--close do runtime Bun
python3 tests/integration/f2_workspaces.py        # F2-workspaces: IPC/stream/render/limpeza + SDK Bun real
python3 tests/integration/f2_workspaces_panel.py  # F2-workspaces: painel real, clique no + e nas abas
python3 tests/integration/f2_launcher.py          # F2-launcher: .desktop/stream/SDK, busca digitada e clique real
python3 tests/integration/supervisor_lifecycle.py # kill -9 do compositor derruba os filhos
python3 tests/integration/supervisor_real.py      # fluxo real shell privilegiado
python3 tests/bench/run_frame_bench.py            # benchmark de protocolo (VM ruidosa: sem conclusão)
```
Para o harness de popup (`popup_client.py`), compile antes com
`cargo build -p aeroshell-compositor --features bench` e
`cargo build -p aeroshell-compositor --example popup_client` — as sondas `POPUP_*` só existem com `bench`.
Para o harness F1i (`f1i_protocols.py`), compile com
`cargo build -p aeroshell-compositor --features bench`,
`cargo build -p aeroshell-compositor --example f1i_client` e
`cargo build -p aeroshell-shell --example frame_webview`. Ele sobe o headless com
`AERO_TEST_INPUT_SOCK`, lista os globals, usa o serial de um `button` sintético num token de
`xdg_activation` real e captura o `foot` à parte; as sondas `TI_FOCUS`/`XDG_ACTIVATE` só existem com `bench`.
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
 