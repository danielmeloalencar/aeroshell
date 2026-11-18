# Progresso

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

### Dívidas técnicas (pré-requisitos da Fase 1)
- **(a) Supervisor ainda é script Python de teste.** `tests/integration/launch_manual.py` e
  `supervisor_real.py` criam o socketpair e lançam os processos **fora do repositório de código**.
  O desenho final é o **supervisor dentro do compositor**, sem variáveis de ambiente para
  privilégio. Enquanto isso não existir, o caminho de produção é validado, mas não é o desenho
  final.
- **(b) Ciclo de vida dos filhos.** Se o compositor morre ou a conexão cai, os filhos devem
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

### SPIKE de role: ligar WebView ao `aero_frame_v1` (pesquisa, SEM implementação)
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
