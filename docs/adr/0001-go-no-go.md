# ADR 0001 — Go/No-Go do S5 e arquitetura de frames

- Status: proposto (aguardando decisão; S3 **não iniciado**)
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

## Dados (PSS em kB; 3 repetições)

| Configuração | WebKitWebProcess | PSS rep1 / rep2 / rep3 | Criação de frame | Risco / complexidade |
|---|---|---|---|---|
| N=1 sem related-view | 1 | 276041 / 276025 / 276035 | 286–295 ms (1ª view) | baixo / trivial |
| N=5 sem related-view | 5 | 439141 / 440132 / 440134 | 138–276 ms por view | médio (5 processos) |
| N=20 sem related-view | 20 | ver nota / 1008996 / 1011027 | 145–286 ms por view | **alto (20 processos)** |
| N=1 com related-view | 1 | 275883 / 277478 / 276140 | 242–282 ms | baixo / trivial |
| N=5 com related-view | 1 | 322738 / 325244 / 327746 | 118–311 ms (1ª) + 117–152 ms | baixo |
| N=20 com related-view | **1** | 506795 / 499574 / 504705 | 1ª 229–334 ms; demais 50–175 ms | baixo / trivial |
| Atlas 1 WebView, 20 frames | **1** | 310021 / 304482 / 303983 | inserir 20: 0,2 ms; remover/reinserir 1: 0,01 ms | médio (precisa src-rect) |

Nota: a repetição 1 de N=20 sem related-view amostrou 0 processos no instante da leitura
(amostragem fora da janela estável); as repetições 2 e 3 são as válidas. PSS por processo
exemplo (N=20 com related, rep2): `aeroshell-shell 187455`, `WebKitNetworkProcess 19100`,
`WebKitWebProcess 293020` → soma `499575`.

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

## Decisão
Aguardando o usuário. S3 permanece bloqueado.
