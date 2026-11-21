# ADR 0001 — Go/No-Go do S5 e arquitetura de frames

- Status: **ratificado pelo relatório F0 em 2026-11-21** (aceito provisoriamente pelo usuário em 2026-11-18; S3 liberado após os ajustes abaixo)
- Data: 2026-11-18
- Ambiente: VirtualBox sem 3D, build `--release`, WebKitGTK 2.52.6, Smithay 0.7.0,
  gtk4-layer-shell 0.8.0 / lib C 1.3.0. **Números de VM não são representativos em GPU real.**

## Contexto
A arquitetura original previa WebViews por frame de janela. Antes de fixar isso, medimos
experimentos sem `aero_frame_v1`, com o mesmo `WebContext`, após `load-changed=Finished` de
todas as views + 3 s de estabilização, 3 repetições por configuração. Métrica principal: **PSS**.
O compositor **não** entrou nas somas (apenas `aeroshell-shell` + filhos WebKit).

Experimento A: N WebViews. Experimento B: 1 WebView com 20 componentes de frame (barra de
título + bordas em faixas, grade sem sobreposição), simulando um atlas.

## Método final (v2)
`tests/bench/run_s5_bench.sh`: espera `load-changed=Finished` de todas as views, faz polling de
PSS até 3 leituras consecutivas com variação < 2%, e registra o tempo de estabilização
(1–3 s nas execuções). Amostra vazia/falha é **registrada e repetida** — nunca descartada em
silêncio: `N20-related rep=1 tentativa=1` teve 80 leituras vazias, foi marcada `FALHA` e
`REPETINDO`, e a tentativa 2 estabilizou em 2 s com PSS 508554 kB.

## Dados (PSS em kB; 3 repetições, método v2)

| Configuração | WebKitWebProcess | PSS rep1 / rep2 / rep3 | Criação de frame | Risco / complexidade |
|---|---|---|---|---|
| N=1 sem related-view | 1 | 275937 / 275967 / 276173 | 286–295 ms (1ª view) | baixo / trivial |
| N=5 sem related-view | 5 | 439349 / 439614 / 439418 | 138–276 ms por view | médio (5 processos) |
| N=20 sem related-view | 20 | 1009834 / 1011756 / 1010505 | 145–286 ms por view | **alto (20 processos)** |
| N=1 com related-view | 1 | 276047 / 276095 / 275912 | 242–282 ms | baixo / trivial |
| N=5 com related-view | 1 | 325401 / 327848 / 325536 | 118–311 ms (1ª) + 117–152 ms | baixo |
| N=20 com related-view | **1** | 508554 / 511498 / 504604 | 1ª 229–334 ms; demais 50–175 ms | baixo / trivial |
| Atlas 1 WebView, 20 frames | **1** | 308178 / 308071 / 308259 | inserir 20: 0,2–0,4 ms; 1 frame: 0–0,01 ms | médio (precisa src-rect) |

PSS por processo exemplo (N=20 com related, v1 rep2): `aeroshell-shell 187455`,
`WebKitNetworkProcess 19100`, `WebKitWebProcess 293020` → soma `499575`.

Superfície do atlas: `1000x613`, 4 bytes/px → **2.452.000 bytes** de buffer (sem cópia de CPU
por frame; o atlas é uma única textura).

## Achados
1. `WebContext` **não** define o modelo de processos no WebKitGTK 2.52. A documentação
   instalada (`/usr/share/gir-1.0/WebKit-6.0.gir`, propriedade `related-view`, linhas
   28105–28116) diz: "The related #WebKitWebView used when creating the view to share the
   same web process and network session. ... only valid during the object construction."
   Confirmado empiricamente: com `related-view` na 1ª view, N=20 gera **1** WebKitWebProcess
   em vez de 20.
2. **Pool reduz latência, não memória.** A criação da primeira WebView custa 230–330 ms e as
   seguintes 50–175 ms quando há `related-view`. A economia de memória vem do
   compartilhamento de processo (`related-view`), não do pool.
3. Atlas é o menor PSS (~304–310 MB) e o DOM é ordens de grandeza mais rápido, mas exige
   composição por sub-região. O Smithay 0.7 já suporta: `RenderElement::draw` recebe
   `src: Rectangle<f64, BufferCoords>` (`smithay-0.7.0/src/backend/renderer/element/mod.rs:386-393`)
   e `WaylandSurfaceRenderElement` repassa esse `src` a `render_texture_from_to`
   (`.../element/surface.rs:381-398`). **Não implementado.**

## Baseline do `aeroshell-shell` (smaps por mapeamento, painel aberto)
PSS total do processo = **109744 kB** nesta execução (o valor ~187 MB visto antes incluía
20 WebViews related). Composição:

```text
   27583 kB  libLLVM (Mesa/WebKit)
   26644 kB  heap próprio
   18329 kB  WebKit/JSC
   14872 kB  Mesa/GL/Vulkan
    9536 kB  anon/deleted
    6616 kB  outras libs compartilhadas
    5348 kB  GTK/GLib
     816 kB  binário
```

Bibliotecas compartilhadas ≈ **73 MB**; heap+anon ≈ **36 MB**. O maior bloco isolado é
`libLLVM.so.21.1` (~27,6 MB), puxado pelo stack Mesa/Vulkan (inclui lavapipe); não há forma
simples de removê-lo sem recompilar WebKitGTK/Mesa, e neste VM `WebKitGPUProcess = 0`.
Heap próprio é majoritariamente alocação de objetos GTK/WebKit; abrir o `WebContext` sob
demanda pode reduzir alguns MB, mas não é relevante frente a 500 MB do WebProcess em N=20.

## Recomendação proposta (NÃO ratificada)
- **Adotar `related-view` para todas as WebViews de frame** — é a decisão de maior impacto e
  custo de implementação quase zero (uma propriedade no builder).
- **Tratar o pool como otimização de latência**, não como requisito de memória; dimensionar o
  pool pelo custo de criação medido, não pelo PSS.
- **Manter o atlas como plano B**, não como escolha padrão: ganha ~195 MB em N=20 neste VM,
  mas adiciona composição por src-rect, hit-testing por região e invalidação parcial.
- **Não registrar "pool obrigatório"** com base neste VM. Repetir N=20 com related-view em
  GPU real antes de fechar o desenho de frames. Se lá o PSS/latência satisfizerem, follow-up
  é S5 com protocolo e pool; se não, reabrir o atlas.

## Decisão do usuário (provisória) e ajustes obrigatórios

1. **`related-view` é o padrão de frames.** Uma **view-âncora oculta** deve ser mantida viva
   durante toda a sessão para que novos frames possam ser criados com o mesmo processo.
   Teste real (`AERO_BENCH_ANCHOR=5`): antes de destruir a âncora, 1 `WebKitWebProcess`
   (pid 84087); 5 s **depois** de `anchor_window.close()`, o mesmo pid 84087 continuava vivo e
   as 5 views related seguiam funcionando. Interpretação: `related-view` é
   **construct-only** — a âncora precisa existir no momento da criação de cada view; destruí-la
   **não** mata as views já criadas, mas impede criar novas views relacionadas a ela. Por isso
   a âncora é mantida viva até o fim da sessão, não por dependência das views existentes.
2. **Pool pequeno com pré-aquecimento (2–4 frames ociosos)** para mascarar latência de criação.
   O pool **não** é descrito como redução de memória; essa é papel de `related-view`.
3. **Atlas é Plano B**, com gatilho objetivo a medir em **GPU real**:
   - PSS adicional dos frames com **10 janelas abertas** > **150 MB** (linha de base do atlas
     medida no VM: ~304 MB para 20 componentes); e/ou
   - latência de resize de janela com frame (do resize do cliente até o frame atualizado)
     **p95 > 16,7 ms** (1 frame de 60 fps) na mesma condição.
   Se qualquer gatilho disparar em hardware real (ou VM trixie com 3D), reavaliar o atlas.
4. **Aviso obrigatório**: dados de **VM sem 3D**; `WebKitGPUProcess = 0`; revalidar em hardware
   real ou VM trixie com 3D antes de fechar o desenho de frames.

Status: ratificado pelo relatório F0 em 2026-11-21; S3 liberado. O ADR será revisitado após S5 com o protocolo.
