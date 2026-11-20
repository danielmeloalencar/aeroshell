# RUNBOOK.md — como compilar, rodar e testar
 
Se um comando abaixo estiver desatualizado, CORRIJA este arquivo na mesma tarefa em que você descobriu.
 
## Pré-requisitos
- Rust via rustup (versão em `rust-toolchain.toml`), Bun, `libwebkitgtk-6.0-dev`, `libgtk-4-dev`, `libgtk4-layer-shell-dev`, `foot`.
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
 
## Testes de integração (precisam de display; rodar à mão, não no CI)
```
python3 tests/integration/frame_resize_edges.py   # 8 arestas/cantos
python3 tests/integration/frame_buttons.py        # max/restaurar/fechar
python3 tests/integration/frame_window_id.py      # 2 janelas, cada frame com o próprio window_id
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
- Protocolo Wayland do frame: `protocols/aero_frame_v1.xml` (PROTEGIDO) + `crates/aeroshell-compositor/src/protocols/aero_frame.rs`
- Grabs de mover/resize: `crates/aeroshell-compositor/src/grab.rs`
- Supervisor/filhos: `crates/aeroshell-compositor/src/supervisor.rs`
- Frame WebView: `crates/aeroshell-shell/examples/frame_webview.rs`
- CSS base de todas as superfícies: `assets/shell-reset.css` (servido por `aero://`)
- IPC e tipos: `crates/aeroshell-ipc`
 