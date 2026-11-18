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

**Bug corrigido**: o primeiro teste com `zenity` conectou ao socket mas não showed nada dentro da
janela — o processo filho herdava `DISPLAY=:0` e o GTK3 abria no X11 do host. O spawn agora faz
`env_remove("DISPLAY")` + `GDK_BACKEND=wayland`. Confirmação visual do usuário: diálogo com cantos
arredondados apareceu dentro da janela do AeroShell, com transparência fora do arredondamento.

**Riscos observados**: `--size` define o modo do output, não redimensiona a janela host (a API
winit do Smithay 0.7 não expõe `set_size` no backend; `Resized` continua funcionando na direção
janela → output). Multi-output em F0 cria globals e modos, mas renderiza apenas o primeiro output;
multi-output real entra em F1.

### S2 — concluído (2026-11-18)
- **Confirmação visual do usuário**: painel verde com cantos arredondados renderizado dentro da
  janela do compositor, na base do output.
- **Causa raiz do primeiro "só cinza"**: eu estava desenhando as layer surfaces manualmente com
  coordenadas físicas erradas (e `Transform::Flipped180`). Corrigido para usar `LayerMap` do
  Smithay (`map_layer` + `arrange`), que é a API responsável por geometry/empilhamento/transform.
- [x] `aeroshell-shell` com `gtk4 0.11`, `webkit6 0.6.1` e `gtk4-layer-shell 0.8.0`.
- [x] Painel ancorado na borda inferior, `exclusive_zone = 48` confirmada pelo compositor.
- [x] WebView com `set_background_color(RGBA(0,0,0,0))` (alpha 0.0) e CSS sem fundo opaco.
- [x] Handler `window.webkit.messageHandlers.aero.postMessage` registrado (ponte S4).
- [x] Compositor agora configura o tamanho da layer surface e a compõe acima das janelas.
- [x] `tests/integration/s2_panel.sh` verde.
- [x] Ordem de link investigada: nesta combinação crate/lib a falha silenciosa não reproduziu;
      `LD_PRELOAD` mantido como mecanismo defensivo documentado em `docs/nested-dev.md`.

**Nota de ambiente**: `sudo` exige senha neste shell, então os headers foram instalados em
`/tmp/opencode/aptroot` via `apt-get download` + `dpkg-deb -x`, com `PKG_CONFIG_PATH` local.
Em máquina normal: `sudo apt-get install -y libgtk4-layer-shell-dev libgbm-dev libseat-dev
libpam0g-dev dbus`.

### Fechamento do S2 (ajustes pedidos)
- Painel agora `Anchor(BOTTOM | LEFT | RIGHT)`, sem `set_width_request`. Geometria real do
  `LayerMap`: `Rectangle { x: 0, y: 602, width: 1000, height: 48 }` para um output 1000x650.
- CSS do painel sem margem horizontal (`margin: 4px 0 0 0`) para eliminar a linha escura na
  borda direita; `border-radius: 10px 10px 0 0`.
- `--size`: winit 0.30 não expõe `Window::set_size`; `set_min/max_inner_size` não reduziu a
  janela host (continua 1280x800), então a janela real é 1280x800 mas o **modo do output** é
  1000x650 e `WinitEvent::Resized` agora é ignorado quando `--size` foi forçado. Log:
  `requested=Some((1000,650)) window=1280x800 mode=1000x650`.
- Timestamps 05:49:43 (execução com `--size 1000x650`, log `report-comp.log`) e 05:50:24
  (execução **sem** `--size`, log `report2-comp.log`) eram de execuções diferentes.
- `LD_DEBUG=libs`: `libwayland-client` inicializa antes de `libgtk4-layer-shell` mesmo com
  `LD_PRELOAD`, na lib C 1.3.0. Falta testar 1.0.x (Debian trixie); defesa mantida.
- RAM release, 1 painel (VM sem 3D): shell 198568 kB RSS, WebKitNetworkProcess 55252 kB,
  WebKitWebProcess 190528 kB; soma RSS 444348 kB, PSS 236891 kB.

### Correção do scrollbar do painel
O "1 px escuro" era **overlay scrollbar**, não margem. Causa: `#panel { height:100%; margin:4px }`
(numos elementos 48 px) somado ao conteúdo — com overflow forçado o painel chegou a
`scrollHeight=162 / clientHeight=48`, `scrollWidth=1300 / clientWidth=1280` e `overflow:visible`.
Criado `assets/shell-reset.css`, servido por `aero://shell-reset.css` e injetado em todas as
superfícies. Com reset: `overflow:hidden`, `scrollHeight=48=clientHeight` mesmo com divisor
de 120 px forçado. Documentado em `docs/sdk-reference/shell-reset.md` e `docs/architecture.md`.
Bug de roteamento do esquema corrigido: `aero://panel/index.html` tem host `panel`, path
`/index.html`; o handler agora casa pela URI completa (antes devolvia corpo vazio → janela cinza).

### Micro-benchmark S5 sem protocolo (antes do S3, N WebViews, mesmo WebContext)
| N | WebKitWebProcess | RSS soma | PSS soma | criação por WebView | destruição |
|---|---|---|---|---|---|
| 1 | 1 | 489800 kB | 286553 kB | 262,6 ms | 1,07 ms |
| 5 | 5 | 1180316 kB | 440440 kB | 300/215/205/205/207 ms | 4,61 ms |
| 20 | 18 | 1149772 kB | 270146 kB | ~149–310 ms | 11,64 ms |

O benchmark inicial (sem `related-view`, uma repetição) foi **superseguido**: ver abaixo e
`docs/adr/0001-go-no-go.md`.

### Benchmark refeito (3 repetições, PSS, 3 s após Finished de todas as views)
Dados brutos em `/tmp/opencode/s5-bench-results.txt`, reproduzíveis via
`tests/bench/run_s5_bench.sh`. Sem `related-view`, N=20 → 20 `WebKitWebProcess` e PSS
~1009–1011 MB; **com `related-view`, N=20 → 1 WebKitWebProcess e PSS ~500–505 MB**; atlas de
1 WebView com 20 frames → 1 WebProcess, PSS ~304–310 MB, inserção de 20 componentes 0,2 ms,
remover/reinserir 1 componente 0,01 ms, buffer 1000x613 ARGB8888 = 2.452.000 bytes. Pool reduz
**latência**, não memória; o ganho de memória vem de `related-view`. Smithay 0.7 já aceita
`src` rect no `RenderElement::draw`.

### S5 protocolo — código gerado e testes (ANTES do desenho do frame)
- `protocols/aero_frame_v1.xml` v1 revisado:wl_surface no get_frame, frame_requested no manager,
  objects wl_surface/wl_seat/wl_region, destructors, enums `state`/`resize_edge`/`error`,
  `configure`/`ack_configure`, `closed`, `set_insets` + `set_shadow_extents` convenção com
  diagrama ASCII, width/height sempre do conteúdo (0 = cliente decide), window_id não reutilizado,
  validação de serial documentada, comportamento em minimized/fullscreen.
- Código gerado com `wayland-scanner 0.31.11` + `wayland-backend 0.3.17`
  (servidor no compositor, cliente em `aeroshell-frame-client`).
- Segurança: global `aero_frame_manager_v1` com `can_view` que só libera clientes cujo PID foi
  registrado como shell privilegiado no `insert_client` (`AERO_SHELL_PID`); padrão é negar todos.
- `unknown_window`: objeto é criado e `closed` enviado (sem erro fatal); erro fatal reservado a
  `already_constructed` (e role, quando aplicado).
- Testes: `tests/integration/aero_frame_protocol.sh` (global invisível a cliente comum; corrida
  com janela inexistente → closed) + 4 testes unitários (unknown/already/known/serial).
- ADR 0003: `ext-background-effect-v1` existe no wayland-protocols 1.47; request privado fica
  como fallback documentado.
- **NÃO implementei o desenho do frame**, conforme pedido.

### S3 — concluído (2026-11-18)
- `data-aero-interactive` → rects enviados por `window.webkit.messageHandlers.aero` → host aplica
  `gdk_surface_set_input_region`. `MutationObserver` + `ResizeObserver` recalculam a região quando
  o texto/tamanho do elemento muda (isso corrigiu o cursor invadindo o texto ao lado do botão).
- Cursor: hit-test marca `cursor_over_interactive`; backend winit troca entre `Pointer` e `Default`.
- Compositor faz hit-test de layer surfaces pela **geometry do `LayerMap`** (não `layer_under`, que
  usa o tamanho bruto do cliente e falha quando a largura é 0 por âncora left+right); respeita a
  input region enviada pelo cliente (confirmado: `input_region = Some(RegionAttributes { rects:
  [(Add, Rectangle { x: 12, y: 6, width: 155, height: 36 })] })`).
- Confirmação manual do usuário: cursor vira mão sobre o botão; contador sobe; área transparente
  não gera mensagem de clique (log com apenas 2 cliques, ambos no botão).
- Ferramenta de ponteiro sintético criada em `AERO_TEST_INPUT_SOCK` (`tests/integration/s3_input.sh`),
  mas a validação final foi manual: a conversão global↔local com `Transform::Flipped180` tornou o
  teste automatizado pouco confiável. Não foi usado como critério de aceite.
- Limitação conhecida: clique na área transparente não altera o foco da janela de trás — a política
  de foco/janela entra no F3.
- `--size` + `Transform::Flipped180` desloca/espelha o painel no framebuffer 1280x800; removido dos
  testes S3 (limitação documentada em `docs/nested-dev.md`).

### ADR 0001 aceito provisoriamente (2026-11-18)
- `related-view` padrão com âncora oculta viva; teste real: destruir a âncora **não** matou o
  WebProcess compartilhado (pid 84087 antes e 5 s depois); a propriedade é construct-only, então
  a âncora só é necessária para criar novas views.
- Pool 2–4 frames pré-aquecidos, **latência**, não memória; atlas como Plano B com gatilho em
  GPU real: PSS adicional dos frames com 10 janelas > 150 MB e/ou p95 de resize > 16,7 ms.
- Benchmark v2 com polling de estabilização (<2% em 3 leituras): falha registrada e repetida,
  nunca descartada. Números finais no ADR.
- Baseline `aeroshell-shell`: PSS 109744 kB, dos quais ~73 MB libs compartilhadas e ~36 MB
  heap/anon; maior bloco libLLVM 27583 kB.

### Pendente
S3 (liberado), S4, S5, S6, S7.
