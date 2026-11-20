# Histórico. O estado atual está em docs/STATE.md.

## F0 — spikes go/no-go

### Concluído
- Verificação de ambiente e versões (2026-11-18): Ubuntu 26.04.1, Rust 1.100.0, Bun 1.4.4,
  GTK 4.22.4, WebKitGTK 2.52.6, wayland 1.24.0, wayland-protocols 1.47.
- Dependências do sistema agora instaladas **globalmente**: `pkg-config --modversion
  gtk4-layer-shell-0` → `1.3.0` sem `PKG_CONFIG_PATH`/`LD_LIBRARY_PATH`/`RUSTFLAGS` customizados.
  O aptroot temporário em `/tmp/opencode/aptroot` não é mais necessário para compilar.
- Verificação limpa (sem env overrides):
  - `cargo check --workspace --all-targets` → `Finished ... in 34.19s` (exit 0).
  - `cargo test --workspace` → todos os suites `ok` (exit 0).
  - `cargo clippy --workspace --all-targets -- -D warnings` → `Finished ... in 0.59s` (exit 0).
  - `bun test` → 1 arquivo / 4 expects verdes.
- `scripts/doctor.sh` criado: verifica ferramentas e pkg-configs e imprime **qual pacote apt
  falta**; `just check` passou a chamá-lo antes do `cargo check`.
- ADR 0002 registra a combinação validada (Ubuntu 26.04 + lib C 1.3.0 + crate 0.8.0) e que
  Debian trixie 1.0.x ainda não foi validado (mínimo declarado do crate é 1.0).
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

### Correções do protocolo — CONCLUÍDAS (2ª tentativa, com socketpair sem WAYLAND_SOCKET no compositor)
Causa do abort anterior: `winit` chama `Connection::connect_to_env()`, que consome `WAYLAND_SOCKET`
(`wayland-client/src/conn.rs:47`) e `winit/src/platform_impl/linux/mod.rs:741`. O compositor passou
a usar `AERO_PRIVILEGED_FD`, convertido para `OwnedFd` no início do `main` e removido do ambiente;
a ponta do socketpair é movida para `UnixStream::from(owned_fd)` e inserida com
`ClientState { is_shell: true }`. `WAYLAND_SOCKET` existe **apenas no Command do filho**.

Saída real dos testes (`tests/integration/aero_frame_protocol.sh`):
```text
OK (a): socket público nunca vê o global, mesmo com AERO_SHELL_PID forjado
OK (a2): cliente privilegiado vê o global e completa o roundtrip
OK (b): segundo get_frame gerou already_constructed
OK (c): wl_surface com toplevel + frame gerou erro role
OK (d): move com serial falso ignorado
```

### Métrica do protótipo do frame (DEFINIDA ANTES DE MEDIR)
- **Métrica**: tempo, em ms, entre `frame.configure(serial, w, h)` ser enviado pelo compositor e
  o `wl_surface.commit` do frame com o novo tamanho, **por passo de resize**.
- Estatística: média, p95 e contagem de frames perdidos (passos sem commit até o timeout de
  ack_configure, fixado em 100 ms).
- Execução: build `--release`, 3 repetições, N=5 e N=20 frames, VM sem 3D (medida relativa);
  veredito final em GPU real.

### Blur: esforço (corrigido)
O crate publicado smithay 0.7.0 não tem handler, **mas o git master já tem**
`smithay::wayland::background_effect` (handler + estado double-buffered), e `wayland-protocols
0.32.13` já traz os bindings em `ext::background_effect::v1`. O custo do protocolo é baixo; o
trabalho real é o render offscreen/shader e o damage tracking (issue #1363), com prior art no
niri. ADR 0003 reescrito: esforço **médio**, adiado para depois do protótipo; trocar para uma rev
fixa do Smithay git será avaliado em ADR próprio no fim do F0/início do F1.

### Correções pedidas ao protocolo (histórico)
- `role` implementado com `smithay::wayland::compositor::give_role` + erro fatal `Role`.
- Filtro de global reescrito: **sem PID/env**. `WAYLAND_SOCKET` inserto pelo supervisor marca
  o `ClientState.is_shell=true`; `can_view` só olha essa marca e o padrão é negar. Cliente do
  socket público nunca vê o global (teste (a) passa, inclusive com `AERO_SHELL_PID` forjado).
- **BLOQUEIO**: ao inserir a ponta do socketpair via `UnixStream::from_raw_fd(WAYLAND_SOCKET)`,
  o compositor aborta com `fatal runtime error: IO Safety violation: owned file descriptor
  already closed` após `message too short, object (1), message error(ous)`. Testes (b) e (c)
  não passaram; necessário depurar a posse do fd antes de seguir.
- `set_blur_region` removido do XML; ADR 0003 agora prescribe apenas `ext-background-effect-v1`.

### S3 — STATUS: CONCLUÍDO (commit `8b97ad8`, validado manualmente)
- Input regions derivadas do DOM (`data-aero-interactive`), `MutationObserver`/`ResizeObserver`,
  cursor `Pointer`/`Default`, hit-test via `LayerMap`. Confirmação visual do usuário: cursor mão
  no botão, contador de clique, área transparente não captura clique. Latência de precisão não
  foi medida com número (validação foi funcional/manual), conforme registrado no próprio S3.

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

### Testes de protocolo migrados para `cargo test` (sem display/env/exec)
O script `tests/integration/aero_frame_protocol.py`/`.sh` foi removido. Os 5 casos viraram
testes Rust em `crates/aeroshell-compositor/src/main.rs::protocol_tests`, com `UnixStream::pair()`
no mesmo processo (supervisor embutido): `AeroState::new` empresta o `&'static Display` e
`Harness::pump` chama `display.dispatch_clients` diretamente. O `aero_frame_manager_v1` é exposto
somente ao cliente cujo `ClientState` foi marcado `is_shell=true` no `insert_client`.

Saída:
```text
running 5 tests
test protocol_tests::a_public_client_never_sees_the_manager ... ok
test protocol_tests::move_with_fake_serial_is_ignored ... ok
test protocol_tests::privileged_client_completes_roundtrip ... ok
test protocol_tests::second_get_frame_is_already_constructed ... aero_frame_manager_v1#5: error 1: janela 0 já tem frame
ok
test protocol_tests::surface_with_toplevel_role_is_rejected ... aero_frame_manager_v1#5: error 0: a wl_surface já possui outro role
ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out
```
Observação: `already_constructed`/`role` são erros fatais de protocolo; a biblioteca cliente
encerra a conexão. Os testes verificam o erro no log e o estado do compositor (`frames.len`).

### CI
`.github/workflows/ci.yml`: doctor + fmt (`cargo fmt --all -- --check`) + `cargo clippy --workspace
--all-targets -- -D warnings` + `cargo test --workspace` + `bun test`. **Exigem display** (não
rodam no CI): `s1_nested.sh`, `s2_panel.sh`, `s3_input.sh`, `run_s5_bench.sh` (usam backend winit
e/ou WebView). Os testes `protocol_tests` e `bun test` rodam headless.

### Documentação e licença
`LICENSE` (MIT) na raiz; `Cargo.toml` e `package.json` com `license: MIT`; `README.md` com
estado pré-alfa e comandos nested (avisa que `aero dev` ainda não existe); `plan.md` reduzido a
resumo com link para este arquivo como fonte única de status.

### Métricas CONGELADAS do protótipo do frame (definidas ANTES de medir)
Ajustes aprovados pelo usuário: o número que decide o `ack_configure` atômico é o **descompasso**,
e a latência precisa ir até a tela, não só até o commit. Nada abaixo foi medido ainda.

| # | Métrica | Como obter | Agregação |
|---|---------|-----------|-----------|
| i | ms de `frame.configure(serial,w,h)` → `wl_surface.commit` do frame com o **novo tamanho**, por passo de resize | instrumentar compositor (envio) + handler de `commit` da superfície do frame | média + p95 por passo |
| ii | **quadros com descompasso**: frames apresentados com conteúdo e decoração de tamanhos diferentes, por resize | comparar, no commit, tamanho do buffer de conteúdo vs. insets/decoração reportados | contagem por resize |
| ii-b | quadros apresentados com conteúdo e frames de tamanhos diferentes por resize | comparar tamanho do frame vs. tamanho do conteúdo no mesmo commit | contagem por resize |
| iii | nº de vezes que o timeout do `ack_configure` (100 ms) disparou | timer no compositor sobre `pending_configures` | contagem + descrição do que o usuário veria |
| iv | latência do ponteiro até a nova posição no drag (baseline; grab no compositor) | `AERO_TEST_INPUT_SOCK` motion → atualização de posição no compositor | média + p95 |
| v | **latência até a tela**: commit → próximo frame composto pelo compositor (ou `presentation-time`, se disponível) | timestamp do commit vs. callback de apresentação do backend | média + p95 |

- Protocolo de medição: **release**, **3 repetições**, **N=5 e N=20**, estabilização por polling
  (variação < 2% em 3 leituras), PSS por processo, VM VirtualBox **sem 3D** → valores relativos,
  não comparáveis com GPU real.
- Arquitetura sob teste: frame = WebView filha por janela via `related-view` de uma âncora oculta
  **sempre viva** + pool pré-aquecido de **2 a 4** views. Atlas é Plano B condicionado a ADR.
- Sanidade: número implausível → repetir e investigar antes de relatar; nenhuma amostra descartada
  em silêncio.

### Resultados do protótipo do frame (medidos em 2026-11-18)
Comando: `python3 tests/bench/run_frame_bench.py` (release com `--features bench`,
3 repetições, N=5 e N=20, compositor winit + cliente privilegiado por socketpair real).
**VM VirtualBox sem 3D: valores relativos, não comparáveis com GPU real.** Todas as execuções
registradas; nenhuma amostra descartada em silêncio.

> **STATUS: NÃO VALIDADO para o desenho dos frames.** Estes números são de **protocolo puro**:
> buffer shm sintético, janela simulada (`AERO_FRAME_BENCH_WINDOWS=1`), cliente de teste que
> dá `commit` e `ack` imediatos. O custo real de um resize é o **WebKit reflowar e repintar**
> a página no tamanho novo — isso **não foi medido**. Portanto o ADR 0001 (um WebView por
> janela com `related-view`) **continua sem validação de latência de resize**. Não escrever
> "passou"; escrever "não validado".

| N | amostras | (i) mean_ms | (i) p95_ms | (ii) descompasso | (iii) timeouts |
|---|----------|-------------|------------|------------------|----------------|
| 5  | 15 | 7,429 | 7,921 | 0 | 0 |
| 20 | 60 | 10,617 | 12,717 | 0 | 0 |

- **(i)** medido de `frame.configure` → `wl_surface.commit` com buffer do novo tamanho.
  Intervalos entre execuções do benchmark: N=5 mean 6,4–9,5 ms; N=20 mean 9,9–15,7 ms,
  p95 11,5–29,7 ms. **Alta variabilidade entre execuções** — coerente com VM sem 3D; não
  concluir nada sobre GPU real.
- **(ii)** detector validado: com `AERO_FRAME_CONTENT_MISMATCH=1` (conteúdo 20×12 px menor),
  `descompasso=true` em 1 commit. Nos runs normais, **0 é trivial**: o cliente sintético
  anexa exatamente o buffer do tamanho configurado, então descompasso não tem como ocorrer.
  O leitor de tamanho do buffer foi corrigido: ler **antes** de `on_commit_buffer_handler`,
  que faz `attrs.buffer.take()`.
- **(iii)** contador validado: com ack forçado para 150 ms, `FRAME_TIMEOUT=1`; nos runs
  normais, **0 é trivial**: o cliente responde na hora, nunca estoura os 100 ms. O que o
  usuário veria num timeout real: o frame ficaria no tamanho antigo até o ack chegar.
- **(iv) NÃO ESTABILIZADO.** Tentativa de 3 leituras com variação < 2% falhou em todas as
  repetições (12 leituras cada). Amostras brutas ficam entre ~0,06 e ~2,1 ms, com picos
  de scheduler da VM. Síntese: `ponteiro_ms=nan` no resumo — **não reportar número único**.
- **(v) NÃO MEDIDO**: este compositor não implementa `presentation-time`; não há caminho
  de commit → frame composto instrumentado.
- **Arquitetura WebKit**: `related-view` de âncora oculta + pool pré-aquecido já validados
  no benchmark S5 (sem protocolo). A ligação do `wl_surface` da WebView ao `aero_frame_v1`
  **não foi feita** — exigiria `gdk4wayland` (dependência nova, aguardando aprovação).
  Portanto os números acima são de protocolo puro, não de WebKit real.

### Spike B — ordenação e acesso ao `wl_surface` (PARADO no B.2)
Código: `crates/aeroshell-shell/examples/frame_spike.rs`. Comando:
`cargo run -p aeroshell-shell --example frame_spike` num compositor nested.

**PROVADO (arquivo:linha + execução):**
- `gtk4-layer-shell-0.8.0/src/lib.rs:170-172`: `init_for_window` configura a layer surface
  "**once it is mapped**" e deve ser chamado **antes de realize**.
- Saída crua do spike:
  ```
  SPIKE realize: zwlr_layer_surface_v1=false
  SPIKE map: zwlr_layer_surface_v1=true
  SPIKE present chamado
  ```
  Logo o proxy `zwlr_layer_surface_v1` **não existe no realize**; o role aparece só no map.
  A ordenação `init → realize → associate → present` é viável quanto ao role.

**BLOQUEIO (B.2):** `gtk4_layer_shell::zwlr_layer_surface_v1()` devolve `None` no realize
(quando `associate` precisaria ser enviado) e, mesmo no map, devolve apenas o proxy
`zwlr_layer_surface_v1*`, sem API para obter o `wl_surface*`. Portanto **não dá** para enviar
`associate` pela conexão do GTK usando apenas a dependência atual.

**Dependência necessária (não adicionada — aguardando aprovação, regra 3.10):**
- nome: `gdk4-wayland` (crate gtk-rs), versão `0.11` (alinhada ao `gdk4 =0.11` instalado),
  licença MIT (família gtk-rs), motivo: expor `WaylandSurfaceExt::wl_surface()` e
  `WaylandDisplayExt::wl_display()` para obter o `wl_surface` do GdkSurface no realize e
  despachar `associate` na **mesma conexão** do GTK.
- alternativa sem crate novo: FFI manual declarando
  `gdk_wayland_surface_get_wl_surface` (`/usr/include/gtk-4.0/gdk/wayland/gdkwaylandsurface.h:44`)
  e `gdk_wayland_display_get_wl_display` (`.../gdkwaylanddisplay.h:48`), ambos presentes na
  lib instalada, com `// SAFETY:` em cada chamada. Ainda exigiria `wayland-client` com backend
  foreign na feature `probe-registry`.

**NÃO EXECUTADO:** B.3 (frame ao redor de zenity, move/resize, teclado, closed) e medições.
Parado no B.2 conforme a regra de parada do usuário.

### B.3 — loop real com GTK + `associate` (CONCLUÍDO)
Código: `examples/frame_spike.rs` + `tests/integration/frame_spike_real.py`
(compositor com `AERO_PRIVILEGED_FD`, spike com `WAYLAND_SOCKET`, zenity pelo socket público).

**PROVADO (logs crus):**
- `wl_surface=Some` no **realize** (antes do map) — spike B.3.1.
- `manager global=5 vinculado no guest do GTK`; `associate(window_id=0) ENVIADO no realize`.
- Compositor: `frame associado; frame_requested+configure enviados window_id=0`;
  `frame layer_surface: duplo configure, FORA do LayerMap total_w=300 total_h=225`.
- Spike recebeu `configure serial=1 1280x800` e `configure serial=2 300x225` (duplo configure).
- Cursores: usuário confirmou que **sempre fica seta** no frame (não herda o Pointer do
  LayerMap, `state.rs:185-189`).
- Modelo de posse: Connection derivada de `wl_surface.backend().upgrade()` (MESMO guest
  singleton de `gdk4-wayland`, `wayland_display.rs:83-107`); nenhum `roundtrip`/`dispatch`
  bloqueante — só `MainContext::iteration` no startup + `dispatch_pending`/`flush`.
  Objetos aero destruídos antes do drop da connection.

**OBSERVAÇÕES VISUAIS CONFLITANTES (3 tentativas — PARADO):**
1. frame cinza **por cima** do zenity, cobrindo o botão (~30 px à direita, ~4 px embaixo);
2. após reconstruir, usuário relatou frame **por baixo**, totalmente coberto (insets=0 →
   tamanho igual ao do cliente);
3. teste diagnóstico com margem de 60 px (`AERO_FRAME_TEST_MARGIN`, só debug/bench;
   `total=420x345`, posição `(-60,-60)`): usuário viu **frame pequeno e por cima**.

**TESTE CONTROLADO DE CORES (aceito pelo usuário) — DEFINITIVO:** frame vermelho
(`#ff0000` via CSS no spike) contra zenity cinza. Imagem enviada pelo usuário mostra o
**vermelho POR CIMA do diálogo**, cobrindo a parte superior do zenity (o botão OK continua
visível abaixo). Logo o z-order está **errado**: `custom_elements` do `render_output`
são desenhados na frente dos elementos do space nesta composição, apesar de
`smithay-0.7.0/src/desktop/space/mod.rs:696-705` acrescentá-los ao vetor antes.
O frame também ficou menor que o `total=420x345` configurado — o GTK não aplicou o
`zwlr_layer_surface.configure` completo (não medido ao certo; estou supondo).

**CORREÇÃO APLICADA (aceita pelo usuário, rejeitando draw direto e space-as-custom):**
a lista de render é composta frente-para-trás com as APIs do Smithay — `layer_map` upper
(Top/Overlay) primeiro, cada janela via `AsRenderElements::render_elements` e o render_element
do seu frame **logo após** (abaixo), lower (Background/Bottom) por último — e passada ao
`OutputDamageTracker::render_output`. Fonte: `smithay-0.7.0/src/desktop/space/mod.rs:578-665`
e `element/mod.rs:404-418`. Nenhum draw direto, geometria preservada, damage mantido.

**VALIDADO (gate objetivo, cores):**
- sobreposição = **AZUL** (cliente na frente); vermelho aparece só atrás/na borda — usuário
  confirmou. Z-order correto.
- `set_insets` de verdade: `insets=(0,30,0,0)`, `shadow=0`, `total=320x270` (cliente 320x240
  + 30). Margem diagnóstica removida.
- **Fechar cliente → `closed` → frame destrói:** validado. Hook de destruição do wl_surface do
  cliente (`add_destruction_hook`) envia `closed`, remove o frame/insets/layer surface. Logs:
  `janela cliente destruída: closed enviado ao frame` e `SPIKE: closed recebido`.

**RESOLVIDO — tamanho do buffer (commit `6eab2c9`):** a causa era o `default_size` fixo do
GTK + `gtk4-layer-shell` só aplicar `zwlr_layer_surface.configure` em eixos esticados
(`layer-surface.c` v1.3.0). Correção: o **shell governa o tamanho** — no `aero_frame.configure`
o spike calcula `total = conteúdo + insets + shadow` e chama `set_default_size(total)`, que
dispara `notify::default-width/height` e reconfigura a superfície. Gate objetivo em **2 tamanhos**:
`FRAME_BUFFER_COMMIT buffer=Some((320, 270)) expected=Some((320, 270)) ok=true` (cliente 320x240)
e `buffer=Some((480, 350)) ... ok=true` após resize do cliente para 480x320 (`AERO_SPIKE_RESIZE`).

**VALIDADO — move/resize com serial válido, teclado, 2 janelas (cliente real, não unitário):**
- `tests/integration/frame_spike_real.py` (ponteiro sintético via `AERO_TEST_INPUT_SOCK`):
  `OK move: clique real na titlebar -> serial válido aceito` (serial=6);
  `OK resize: clique real na alça -> serial válido aceito` (serial=9, `ResizeEdge::BottomRight`);
  `OK teclado: foco foi ao cliente @16, nunca ao frame @17`;
  `OK closed: compositor enviou e spike recebeu`. `GATE DE TAMANHO: PASSOU (2 tamanhos + closed)`.
  A alça de resize vive na titlebar (últimos 16 px à direita): com insets só no topo, o canto
  inferior direito do frame cai dentro do conteúdo do cliente, que roteia o input primeiro.
- Correção de mecanismo no `surface_under` (frame): o ponto do tuple de foco deve ser a
  **origem global** da superfície, não o hit local — Smithay calcula `event.location - loc`
  (`smithay-0.7.0/src/input/pointer/mod.rs:795`). Sem isso o move não iniciava.
- `tests/integration/frame_spike_two.py` (2 clientes azuis `AERO_SPIKE_BLUE_N=2` + 2 frames
  `AERO_SPIKE_FRAME2=1`): `SANIDADE 2 JANELAS: PASSOU`.
  (a) 2 associates + 2 buffers `(320, 270) ok=true`;
  (b) ordem frente->trás `[client@43, frame@45, client@16, frame@17]` — cada frame LOGO APÓS
  (abaixo) do seu cliente;
  (c) raise do cliente 0 → `[client@16, frame@17, client@43, frame@45]`; raise do cliente 1 →
  volta. O frame **segue** o seu cliente no empilhamento;
  (d) `focus_true=['16','43']` — teclado nunca nos frames `@17,@45`.
- Ordem de render logada só quando muda (`backend/winit.rs`, `state.last_render_order`).

**Verificação:** `bash scripts/check.sh` → `OK: scripts/check.sh passou`
(clippy `-D warnings`, `cargo test` 14 testes no compositor incl. 7 de protocolo, `bun test` 1 pass).

**Não iniciado:** C (WebView real, related-view/pool, métricas, presentation-time).

**Ressalva de ambiente:** tudo acima em nested (winit) no VirtualBox **sem 3D**; valida
FUNCIONAMENTO, não desempenho. Latências S5 (~11–13 ms) são indicativas, não benchmark de GPU real.

### Supervisor interno + ciclo de vida dos filhos — CONCLUÍDO (2026-11-18)

Dívidas (a) e (b) abaixo resolvidas. ADR 0005 registra a decisão (rustix 1.1.6; rota única).
Escopo **shell apenas**; lock/greeter seguem para F5/F6. Nenhuma fase avançada.

**O que foi feito**
- `crates/aeroshell-compositor/src/supervisor.rs` (novo): cria o `UnixStream::pair()` dentro do
  compositor, insere a ponta servidor com `is_shell=true` e entrega a ponta do filho via
  `WAYLAND_SOCKET`. Privilégio pela ponta do socket, nunca por PID/env (§3.5).
- `PDEATHSIG=SIGKILL` no `pre_exec` de cada filho (rustix), com guarda contra a corrida
  fork/morte-do-pai (`getppid() != parent_pid` → `_exit`). CLOEXEC limpo só no filho privilegiado.
- `--command` (cliente público) também rastreado pelo supervisor com `configure_lifecycle`
  (PDEATHSIG sem privilégio). Shutdown gracioso: SIGTERM → poll → SIGKILL → reap.
- CLI `--shell`/`--shell-arg`/`--shell-env`; `AERO_PRIVILEGED_FD` removido do compositor.
- Harnesses migrados para a rota única: `frame_spike_real.py`, `frame_spike_two.py`,
  `supervisor_real.py`, `launch_manual.py`, `run_frame_bench.py`. Novo `supervisor_lifecycle.py`.

**Verificações (comando → saída → interpretação)**
- `bash scripts/check.sh` → `OK: scripts/check.sh passou` (doctor + fmt + clippy `-D warnings` +
  `cargo test --workspace` [17 testes de protocolo ok] + `bun test` [1 pass]). Gate verde.
  A 1ª execução do `bun test` falhou por carga (12.2 s, vários compositores ativos); repetido
  isolado passou (4.3 s) — flake de timing registrado, não descartado.
- `cargo test -p aeroshell-compositor supervisor::` → `3 passed; 0 failed`
  (fd sobrevive ao exec; env isolado `DISPLAY`/`AERO_PRIVILEGED_FD` removidos; shutdown reap).
- `python3 tests/integration/supervisor_lifecycle.py` → `SUPERVISOR: PASSOU`. Fase 1: shell vê
  `aero_frame_manager_v1`, cliente público não vê. Fases 2 e 3: `kill -9` no compositor mata
  tanto o shell (`--shell`) quanto o cliente público (`--command`) via PDEATHSIG.
  **Teste escrito primeiro e visto falhando**: antes do roteamento do `--command`, a Fase 3
  reportou `cliente --command 20620 SOBREVIVEU ao kill -9 do compositor (orphan)`.
- `python3 tests/integration/frame_spike_real.py` → `GATE DE TAMANHO: PASSOU (2 tamanhos + closed)`
  (320x270 e 480x350 com `buffer == expected ok=true`; move/resize com serial válido; teclado
  nunca no frame; closed após morte do cliente).
- `python3 tests/integration/frame_spike_two.py` → `SANIDADE 2 JANELAS: PASSOU` (2 associates,
  2 buffers ok=true; cada frame logo abaixo do seu cliente; raise mantém o par; teclado nos
  clientes @16/@43, nunca nos frames @17/@45).
- `python3 tests/integration/supervisor_real.py` → `OK: fluxo real validado` (shell real com
  `--features probe-registry`: registry `aero_frame_manager_v1 <<< privileged` via WAYLAND_SOCKET;
  painel layer-shell; S3 hit-test). O teste passou a compilar o shell com a feature do probe.
- `python3 tests/bench/run_frame_bench.py` (release) → N=5: 15 amostras, `frames_ok=15`,
  `descompasso=0 timeouts=0`, mean 26.255 ms / p95 30.651 ms; N=20: 60 amostras, `frames_ok=60`,
  mean 21.923 ms / p95 27.480 ms. Provas: (iii) ack 150 ms → `FRAME_TIMEOUT=1`; (ii) conteúdo
  menor → `descompasso=true em 1 commit`. **Rota única funciona no bench.**

**Não foi feito / não foi testado**
- Latência do ponteiro no bench **não estabilizou** (variação < 2% em 3 leituras nunca atingida
  em 12 polls); `ponteiro_ms=None`, amostras registradas. "Não medido" nesta VM sem 3D.
- Números de desempenho são **relativos à VM sem 3D** (VMware SVGA3D/LLVM), não comparáveis a
  GPU real. A inversão N=20 (21.9 ms) < N=5 (26.3 ms) não foi explicada — não concluir nada
  sobre desempenho a partir desta execução.
- Lock/greeter (processos autônomos) permanecem em F5/F6; o supervisor atual cobre o shell.
- Dívida (c) presentation-time e (d) socket órfão continuam pendentes.

**Riscos / incertezas**
- Estou supondo que o `pre_exec` async-signal-safe é suficiente; o caminho de erro não aloca
  (`io::Error::from_raw_os_error`), mas isso não foi auditado sob sinal real concorrente.
- O `PDEATHSIG` cobre a morte do compositor; a morte de um filho intermediário (ex.: `sh -c`
  sem `exec`) pode orphanar netos — o harness usa `exec` onde isso importa.

### Pendências menores registradas (2026-11-19 — SEM implementar agora)

**1. Auditoria do closure `pre_exec` (linha a linha).** Código atual
(`crates/aeroshell-compositor/src/supervisor.rs:116-148`):
```rust
cmd.pre_exec(move || {
    parent_death_guard(parent_pid)?;                 // getppid + prctl(PR_SET_PDEATHSIG)
    let fd = BorrowedFd::borrow_raw(raw_fd);          // wrap de i32, sem syscall
    fcntl_setfd(fd, FdFlags::empty())                 // fcntl(F_SETFD)
        .map_err(|e| io::Error::from_raw_os_error(e.raw_os_error()))?;
    Ok(())
});
// ...
fn parent_death_guard(parent_pid: Pid) -> io::Result<()> {
    if getppid() != Some(parent_pid) {
        return Err(io::Error::from_raw_os_error(3)); // ESRCH
    }
    set_parent_process_death_signal(Some(Signal::KILL))
        .map_err(|e| io::Error::from_raw_os_error(e.raw_os_error()))
}
```
Conclusão: **async-signal-safe**. Só syscalls `getppid`/`prctl`/`fcntl` (wrappers finos rustix:
`process/id.rs:137`, `process/prctl.rs:66-68` → `prctl_2args(PR_SET_PDEATHSIG, …)`,
`io/fcntl.rs:68`); wraps de tipos `Copy` (`Pid`, `RawFd`, `BorrowedFd`); nenhum `String`/`Vec`/
`Box` capturado. Caminho de erro usa `io::Error::from_raw_os_error`, que empacota o código OS
inteiro com tabela estática — **sem payload Custom/Box** (fonte std `io/error.rs:76-82`:
`const FUNCTIONS: &'static OsFunctions`, "not created at runtime"). Os construtores que alocam
são `Error::new`/`other` (variante Custom), **não usados**. Sem lock de userspace, sem caminho de
pânico (tudo retorna `Result`). Ressalva: `rust-src` não instalado; a variante `Os` inline é o
contrato da API std, não abri o `enum ErrorData` nesta versão.

**2. Dívida: `--command` via `sh -c` pode orphanar netos.** O PDEATHSIG atinge o `sh`; se o
comando não usar `exec`, o neto (ex.: um daemon que o shell forkou) pode sobreviver. Mitigação
futura: lançamento direto (sem `sh`) quando não há shell-expand, ou exigir `exec`. **Registrado,
não implementado.** Os harnesses já usam `exec` onde isso importa (`supervisor_lifecycle.py`).

**3. Bench: VM ruidosa, sem conclusão.** A inversão N=20 (21.9 ms) < N=5 (26.3 ms) e a variação
7–10 ms vs 20–26 ms entre execuções são ruído da VM sem 3D (VMware SVGA3D/LLVM). **Nenhuma
conclusão de desempenho** a partir destes números; latência do ponteiro não estabilizou
(<2% em 3 leituras), registrada como "não medido". Medição válida só em GPU real.

**4. Higiene do commit (verificado).** `git diff origin/master..HEAD --name-status -- docs/adr`
→ apenas `A docs/adr/0005-supervisor-interno-ciclo-de-vida.md` (nenhum ADR aceito 0001–0004
editado). `git diff origin/master..HEAD --name-only | grep target/|/tmp` → vazio (nenhum
artefato de build ou temporário commitado).

### Dívidas técnicas (pré-requisitos da Fase 1)
- **(a) Supervisor ainda é script Python de teste.** — **RESOLVIDO (2026-11-18, ADR 0005).**
  Supervisor dentro do compositor (`supervisor.rs`); `AERO_PRIVILEGED_FD` removido; harnesses
  migrados para `--shell`. Ver seção "Supervisor interno + ciclo de vida dos filhos" acima.
  Histórico: `tests/integration/launch_manual.py` e
  `supervisor_real.py` criam o socketpair e lançam os processos **fora do repositório de código**.
  O desenho final é o **supervisor dentro do compositor**, sem variáveis de ambiente para
  privilégio. Enquanto isso não existir, o caminho de produção é validado, mas não é o desenho
  final.
- **(b) Ciclo de vida dos filhos.** — **RESOLVIDO (2026-11-18, ADR 0005).** PDEATHSIG=SIGKILL
  explícito (rustix) para shell e cliente `--command`; prova `supervisor_lifecycle.py` Fases 2/3
  (kill -9 no compositor mata os filhos). Deixou de ser comportamento incidental do GDK.
  Se o compositor morre ou a conexão cai, os filhos devem
  terminar (fail-secure de sessão). Teste executado: `launch_manual.py` (compositor 160117,
  shell 160120, backdrop 160121, mais WebKitNetworkProcess/WebKitWebProcess), depois
  `kill 160117` (simula fechar a janela). Saída:
  ```
  === ANTES ===
  160117 aeroshell-compositor --backend winit --socket wayland-aero-manual
  160120 aeroshell-shell
  160121 aeroshell-shell
  160181 /usr/lib/.../WebKitNetworkProcess
  160210 /usr/lib/.../WebKitWebProcess
  === DEPOIS (ps aux | grep -E "aeroshell|WebKit") ===
  daniel  4357 ... ptyxis --new-window --working-directory /home/daniel/Projetos/aeroshell
  ```
  **Observação:** os filhos realmente terminaram nesta execução, mas por comportamento
  incidental do GDK ao perder a conexão Wayland — **não** por um mecanismo explícito do
  supervisor. Em Fase 1 isso precisa ser garantido pelo supervisor dentro do compositor.
- **(c) presentation-time** não implementado (métrica v).
- **(d) socket órfão (dívida menor).** O compositor não remove o socket ao sair; sobrou
  `/run/user/1000/wayland-aero-spike` após o spike. Corrigir com `unlink` no desligamento
  gracioso (e tratar SIGKILL como caso separado).

### SPIKE de role: ligar WebView ao `aero_frame_v1` (pesquisa, SEM implementação)
**Status: CONCLUÍDO e implementado.** A opção B abaixo foi aceita como **ADR 0004** e o
`associate` sem role já está em `protocols/aero_frame.rs:136` ("NÃO atribuir role. Apenas
marcar"), validado no B.3. Evidência (arquivo:linha) **reconferida nas versões instaladas em
2026-11-19**: `gdkwaylandsurface.h:44`, `GdkWayland-4.0.gir:446/725`, `Gdk-4.0.gir:32419/32448`,
`gtk4-layer-shell-0.8.0/src/manual.rs:13-26`, `wayland.xml:1418-1423`,
`smithay-0.7.0/src/wayland/compositor/tree.rs:170-177`. Nenhuma mudança de XML; sem `gdk4-wayland`.
Pesquisa em docs/código instalados. **Nenhum XML alterado, nenhuma dependência adicionada.**
Hipótese do usuário confirmada: `get_frame` dá `give_role(aero_frame)` em
`protocols/aero_frame.rs:81`, mas o GDK4/Smithay já deram outro role à superfície ao mapear.

**Evidências (fonte citada):**
- `/usr/share/wayland/wayland.xml:1418` — "A surface can have only one role at a time... Once a
  wl_surface is given a role, it is set permanently".
- `smithay-0.7.0/src/wayland/compositor/tree.rs:170-177` — `set_role` retorna `AlreadyHasRole`
  se já há role diferente. `give_role` é esse `set_role` (`compositor/mod.rs:394`).
- `smithay-0.7.0/src/wayland/shell/wlr_layer/handlers.rs:86` — handler layer-shell dá
  `LAYER_SURFACE_ROLE`. Logo `give_role(aero_frame)` numa `GdkSurface` mapeada como layer
  **falha** (`ManagerError::Role`), e o frame do próprio shell seria rejeitado.
- `/usr/share/gir-1.0/GdkWayland-4.0.gir:726` (`gdk_wayland_surface_get_wl_surface`) e `:447`
  (`gdk_wayland_display_get_wl_display`); `/usr/include/gtk-4.0/gdk/wayland/gdkwaylandsurface.h:44`.
  São as **únicas** APIs Wayland do GDK4 para superfície/display.
- `/usr/share/gir-1.0/Gdk-4.0.gir:32419` (`gdk_surface_new_popup`) e `:32448`
  (`gdk_surface_new_toplevel`) — os únicos construtores de `GdkSurface`. Ambos criam superfície
  com role (popup/toplevel). **Não existe API para criar uma `wl_surface` sem role no GDK4.**
- `gtk4-layer-shell-0.8.0/src/manual.rs:13-22` — expõe o `zwlr_layer_surface_v1` **já existente**
  de uma `gtk::Window`; não cria superfície nova.
- Não há `app_id` por janela no GDK4/Gtk4 instalado (nenhuma entrada `app_id` em
  `GdkWayland-4.0.gir`); `application-id` é do `GApplication`, compartilhado por todas as janelas.

**Respostas:**

| Opção | Viável hoje? | Mudança no `aero_frame_v1` | Esforço |
|---|---|---|---|
| **A** — superfície sem role via gdk4-wayland | **NÃO.** GDK4 não expõe criação de `wl_surface` sem role (só `new_toplevel`/`new_popup`); `get_wl_surface` devolve a superfície já com role. Exigiria `gdk4-wayland` + provável FFI/unsafe e ainda assim não há API para "desenhar numa superfície sem role". | Nenhuma ajuda: o bloqueio é no GDK, não no protocolo. | Alto e incerto — pode ser impossível sem sair do GDK. |
| **B** — frame como `layer_surface` + `associate(surface, window_id)` que só **marca** | **SIM.** O S2 já validou layer surface transparente com WebView e `LayerMap` (`protocols/layer_shell.rs:53-63`). Basta **não** chamar `give_role`: registrar a superfície já existente como frame, manter no `LayerMap` e deixar o compositor definir geometria (o cliente não pede tamanho). | Adicionar request `associate` no manager (sem `give_role`); remover/condicionar `get_frame`. **Muda protocolo congelado → exige aprovação.** | **Baixo/médio** — reaproveita S2; toca no handler de layer e no `get_frame`. |
| **C** — marcar `xdg_toplevel` por `app_id` | **Frágil.** `app_id` é do `GApplication`, igual para todas as janelas; não há API por janela no GDK4 instalado. Compositores e GTK tratam toplevels de forma especial. | `associate` com `app_id` em vez de `window_id`, com risco de colisão entre janelas. | Médio, com risco de retrabalho. |

**Recomendação: opção B.** É a única que reaproveita algo já validado (S2) e resolve a
incompatibilidade de role sem FFI especulativa. Custo: uma mudança no XML congelado (nova request
`associate` no `aero_frame_manager_v1`, removendo a atribuição de role de `get_frame`), que
**depende de aprovação explícita**. Não listo `gdk4-wayland` porque a opção A não é recomendada;
se o usuário escolher A, listarei nome/versão/licença/motivo na ocasião.

**Não executei:** nenhuma implementação, nenhuma medição com WebKit real, nenhuma mudança de XML.
O spike é apenas leitura de documentação/código instalados. Próximo passo depende da decisão.

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

### MODO SPIKE — acertos de role (2026-11-19)
- Regra de role corrigida em `protocols/aero_frame.rs`: `role_allowed` aceita superfície
  sem role OU já com `layer_surface`; rejeita `xdg_toplevel`/`xdg_popup`/`cursor_image`.
- Testes novos (vistos falhando antes da correção): puros `role_none_or_layer_is_allowed`,
  `toplevel_popup_cursor_roles_are_rejected`; protocolo real
  `associate_accepts_layer_role_and_rejects_toplevel_popup_cursor` (give_role no servidor +
  `Client::object_from_protocol_id`). Sinal de rejeição = `connection.protocol_error()` +
  `frames.len()==0` (o evento normal `frame_requested` também marca `protocol_error` no harness).
- Texto do `aero_frame_v1.xml` atualizado (só descrição/summary, sem assinaturas/nomes) para
  refletir a regra; arquivo protegido tocado sob aprovação explícita do usuário (acerto #2).
- `bash scripts/check.sh` verde (1ª execução: flake do S1 nested sob carga, exit 1; isolado e
  re-execução limpa: pass). Nenhum unsafe/unwrap/allow novo.
- Bloqueio do passo 2: `foot` NÃO instalado (nenhum terminal além de `zenity` GTK3); instalar
  foot é dependência de sistema fora do repositório → aguarda decisão.

### MODO SPIKE — decoração server-side + latência do frame (2026-11-19)
- `foot` instalado (1.25.0) e usado como cliente real. Arrasto da barra HTML move a janela
  (grab real do compositor); usuário confirmou "funcionou perfeitamente".
- CSD dupla corrigida: implementado `xdg-decoration` server-side
  (`protocols/xdg_shell.rs`: `XdgDecorationHandler` + `delegate_xdg_decoration!`;
  `state.rs`: `xdg_decoration_state`). `new_decoration`/`request_mode`/`unset_mode` →
  `decoration_mode=ServerSide` + `send_configure`. O foot larga a própria barra; a barra
  HTML passa a ser a ÚNICA decoração, alinhando render (`render_loc = geo.loc -
  geometry().loc`, `backend/winit.rs:169`) com a região arrastável do hit-test
  (`state.rs surface_under`, content_rect = `geo.loc`/`geo.size`). Medido: frame buffer
  696x524 (conteúdo 494 + titlebar 30), `ok=true`.
- Latência da decoração: o `--command` (cliente leve) era lançado ANTES do `--shell`
  (frame_host GTK+WebKit, pesado). Reordenado em `main.rs`: shell primeiro, cliente depois.
  Medido (log, nested winit, VM sem 3D): intervalo foot-map → 1º ack_configure caiu de
  ~2,2 s para ~1,5 s. O residual (~1,5 s) é o cold-start do WebKitGTK (linkagem dinâmica +
  spawn do WebProcess) antes de o frame_host associar; NÃO é atraso do compositor.
- `bash scripts/check.sh` verde (1ª execução: flake do S1 nested sob carga; re-execução
  limpa: pass). Nenhum unsafe/unwrap/allow novo; nenhum arquivo protegido alterado.
- NÃO medido: desempenho em GPU real; latência com pool pré-aquecido de frames (Plano B /
  ADR, deferido). A eliminação do cold-start exige o pool pré-aquecido da arquitetura —
  proposto como próximo passo, aguarda aprovação.

### Cold-start — solução temporária (opção 2, 2026-11-19)
- Decisão do usuário: adiar o 1º cliente (`--command`) até o frame_host associar, só para o primeiro cliente; implementado (`main.rs` adia, `aero_frame.rs` lança no 1º associate e liga o frame ao cliente quando ele mapeia — `bind_pending_frame_client`; configure sai no commit, sem flash de tamanho cheio). Medido: configure 696x494 ~10 ms após o foot mapear; `closed` ok; check.sh verde.
- Pool pré-aquecido fica para DEPOIS da fatia vertical, em passo próprio, com medição final em GPU real.
- Risco assumido (temporário): se o frame_host nunca associar, o 1º cliente não é lançado.

### Passo 1 da fatia vertical — resize pelo mouse (8 bordas/cantos), 2026-11-19
- Hipótese: a inconsistência anterior (só parte das arestas registrava) era o harness de
  input sintético — sem `release` entre presses o grab anterior continuava ativo e consumia
  os eventos; a lógica de arestas estava certa.
- Implementado no frame_host: `BORDER=6`, insets `(6,30,6,6)` e detecção da aresta pela
  posição local do ponteiro (`frame_webview.rs`); `ResizeGrab` cobre as 8 arestas
  (`grab.rs:208-255`), `MIN_W=120`/`MIN_H=80`. Timer de auto-quit de 30 s removido (era a
  causa da decoração sumir).
- Teste novo `tests/integration/frame_resize_edges.py` (nested winit, exige display): nas 8
  posições o hit-test resolve para o frame (surface 21, origem (54,50)) e não para o cliente
  (surface 18, origem (60,80)); a aresta detectada e o grab criado casam com a esperada em
  8/8. Grab completo `press → motion → release` aplica tamanho novo: conteúdo
  696x494 → 732x520. Isso também prova que a release encerra o grab (sem isso só o 1º press
  por rodada registrava).
- `bash scripts/check.sh` verde. Nenhum `unsafe`/`unwrap`/`allow` novo; nenhum arquivo
  protegido alterado; nenhuma dependência nova.
- NÃO testado: serial inválido vindo de um cliente real (coberto só por teste unitário
  `serial_valid`); o harness sintético não consegue produzir serial desencontrado porque o
  serial entregue ao cliente é o mesmo gravado em `last_button_serial`.
- Pendências do marco: botões fechar/maximizar (passo 2), runtime Bun + SDK (passo 3),
  painel React/TSX (passo 4).

### Atraso da decoração no resize — 3 causas candidatas (modo spike, 2026-11-19)
- Assimetria estrutural (lida no código): a POSIÇÃO do frame é recalculada a cada redraw a
  partir da geometria do cliente + `fgeo.offset()` (`backend/winit.rs:193-202`) — por isso
  mover não atrasa; o TAMANHO só muda via protocolo (`configure` → `set_default_size` no GTK →
  repaint do WebKit → commit). O frame depende da cadência de commit do próprio cliente frame.
- Causa 1 (timer fixo de 20 ms, `frame_webview.rs:410-413`): confirmada, mas a substituição
  pedida (`glib::unix_fd_add_local`) NÃO existe no glib fixado 0.22.11 (só `g_source_add_unix_fd`
  comentado no glib-sys). Alternativa viável no crate: `gio::DatagramBased::create_source`, que
  exige `OwnedFd` (dup do fd da conexão) → dependência nova (`libc` 0.2.191 MIT ou `rustix`
  0.38.44/1.1.6 MIT, ambos já transitivos em `Cargo.lock`). Aguarda aprovação; nada foi adicionado.
- Causa 2: JÁ implementada — o compositor reenvia o `configure` do frame a partir da geometria
  COMMITADA do cliente no handler de commit (`protocols/compositor.rs:140-149`,
  `client_content_size` → `element_geometry` → `Window::geometry` = estado cacheado do último
  commit). Evidência no teste: pedido +40/+30, obtido +36/+26 (arredondamento da grade do foot);
  o frame recebeu o tamanho real commitado. Nada alterado.
- Causa 3: `html/body` já era transparente (`assets/shell-reset.css`) e a WebView já tinha fundo
  transparente (`frame_webview.rs:374`). Faltava o fundo do widget GTK: adicionado via
  `CssProvider` + `style_context_add_provider_for_display` (commit 2cf354d). Verificação visual pendente.
- Se o atraso persistir, as duas alternativas de desenho (não implementadas, aguardam decisão):
  1. Nove fatias no compositor: frame como grade de 3x3 superfícies, cada faixa de borda
     independentemente redimensionável, sem depender do repaint do WebKit.
  2. Buffer sobredimensionado com recorte por `src-rect`: o frame commita um buffer grande
     (ex.: +512 px por lado) e o compositor mostra só o `src-rect` do tamanho real, eliminando
     o round-trip de redimensionamento do cliente frame.
  3. Nenhuma das duas foi medida: o atraso atual não foi quantificado após as correções baratas
     (só a reprodução do teste de arestas, sem regressão).
  4. Para quantificar: medir `configure → commit` do frame durante um grab contínuo, com o
     `frame_probes` (feature `bench`) já existente, em GPU real.
  5. Decisão adiada porque o custo (complexidade de hit-test por fatia ou gestão de src-rect)
     não se justifica sem o número; o caminho barato pendente é o despacho imediato (causa 1).

### Passo 2 — botões da barra HTML (2026-11-19, caminho executado pela 1ª vez)
- Caminho medido OK: JS da WebView → `script-message-received` → IPC do compositor. Teste manual
  `tests/integration/frame_buttons.py`, `VEREDITO: OK` — hit-test dos 3 botões resolve para o
  surface do frame (`wl_surface@32`, origem `(54,50)`) e não para o do cliente (`wl_surface@18`);
  o handler de ponteiro do frame_host viu o press nos 3 cliques; `maximize` → conteúdo
  `(696,494)`→`(800,600)`, `maximize` de novo → `(696,494)`, `close` → `list` via IPC = `['.']` e
  `FRAMEWEB: closed recebido`.
- Causas REAIS do fracasso inicial (medidas, não presumidas): (1) `postMessage(JSON.stringify(...))`
  dupla-codifica — `JSCValue::to_json(0)` devolve `"{\"action\":\"ready\"}"`, `serde_json::from_str`
  produz `Value::String` e `.get("action")` falha; corrigido postando objeto JS. (2) o teste enviava
  só `button press`; o `click` do DOM exige press+release. (3) o regex do teste não casava o JSON
  escapado.
- Hipótese retratada (não provada): conectar o sinal sem detail. Reexecutado com `Some("aero")` e o
  teste corrigido dá OK; nada no código mudou além do `postMessage` — o restante está como no
  commit `2cf354d`.
- Interferência do usuário: a primeira medição (`press=False`, sem linha de IPC) é compatível com a
  janela ter sido movida com o mouse real durante a run. Nada foi descartado: o teste foi reexecutado
  sem entrada real e deu `press=True` nos três cliques.
- Limitações não corrigidas: `maximize` não reposiciona o cliente na origem do output
  (`window_ipc.rs:56-125` só muda tamanho); `restore` envia `size = None` (o cliente escolhe); o
  `window_id` do handler vem de `entries.first()` (uma janela no spike).
- NÃO testado: atraso da decoração depois das correções baratas (causa 3 sem verificação visual,
  causa 1 bloqueada por dependência). `bash scripts/check.sh` verde.
