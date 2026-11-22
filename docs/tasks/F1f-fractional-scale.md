# Tarefa F1f — fractional-scale (RASCUNHO — aguarda "ok")

> Subcartão de `docs/tasks/F1-gap-protocolos.md`. Um agente deve executar lendo só AGENTS.md,
> STATE.md, RUNBOOK.md, o cartão-mãe e este arquivo. Uma tarefa por sessão; PARE se o objetivo mudar.
> Aberto depois do gate determinístico da tarefa 00 (2026-11-22).

## Objetivo (uma frase, verificável)
O compositor anuncia `wp_fractional_scale_v1`, reporta a escala preferida às superfícies e **renderiza
cada superfície com a escala dela**, mantendo bordas/titlebar com espessura estável em escalas 1.25/1.5
sem drift de arredondamento, com regressão zero em 1.0.

## Contexto mínimo — premissas CONFIRMADAS na fonte fixada (Smithay 0.7.0)
- `wayland/fractional_scale/mod.rs:76-94` — `FractionalScaleManagerState::new::<D>(&dh)` cria o global
  (versão 1) e exige `D: FractionalScaleHandler + GlobalDispatch/Dispatch` dos dois protocolos.
- `fractional_scale/mod.rs:213-216` — `FractionalScaleHandler` tem um único método, com corpo default:
  `fn new_fractional_scale(&mut self, surface: WlSurface)`.
- `fractional_scale/mod.rs:225-251` — `FractionalScaleStateUserData = RefCell<FractionalScaleState>`;
  `set_preferred_scale(f64)` envia `preferred_scale(round(scale*120))` só quando muda; `preferred_scale()`.
- `fractional_scale/mod.rs:254-264` — `with_fractional_scale(&SurfaceData, closure)` lê/escreve esse estado.
- `fractional_scale/mod.rs:268-281` — `delegate_fractional_scale!(AeroState)` gera os delegates.
- **`WaylandSurfaceRenderElement` já considera a escala da superfície** ao montar elementos via
  `AsRenderElements` (o chamador passa `Scale`); o nosso `render/scene.rs:40` hoje usa **somente**
  `output.current_scale().fractional_scale()` para todas as superfícies — é aqui que entra a correção.
- Repo: `cli.rs:118-124,179-181` já tem `--scale F`; `winit.rs:129` aplica `Scale::Fractional(opts.scale)`;
  **`headless.rs:62-68` não aplica** — necessário para o teste de pixels headless.

## Escopo (mínimo, SEM mudar geometria aprovada)
1. Registrar `FractionalScaleManagerState` em `state.rs`, `impl FractionalScaleHandler` e
   `delegate_fractional_scale!` no módulo de protocolos.
2. Definir a escala preferida de cada superfície a partir do output dela (hook `new_fractional_scale` e
   ponto de atualização quando a escala do output muda), sem `unwrap`/`allow`.
3. **No render (`render/scene.rs`), calcular a escala por superfície** via `with_states` +
   `with_fractional_scale(...).preferred_scale().unwrap_or(output_scale)` e passá-la ao
   `AsRenderElements`/`FrameRenderInput.output_scale` daquela superfície — não trocar a receita do
   9-slice, apenas a fonte da escala.
4. Aplicar `opts.scale` também no output headless (`backend/headless.rs`) para habilitar o teste.
5. Teste-first e harnesses: ver critério abaixo. Log sob `cfg(feature = "bench")`:
   `FSCALE_SURFACE t=… surface=… scale=… source=preferred|output` (nada em produção).

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste-first visto falhando:** harness novo `tests/integration/fractional_scale.py` (escrito antes)
   com `--scale 1.25` e `--scale 1.5` no headless → baseline FALHA (sem global, aviso do `foot`
   "compositor does not implement fractional scaling", e/ou medição de pixels fora da tolerância).
   Depois: `VEREDITO: OK` com `FSCALE_SURFACE … scale=1.25/1.5`.
2. **Pixels:** com a grade do `PIXEL_FRAME` (64×40), medir a espessura/altura da borda e da titlebar em
   1.0, 1.25 e 1.5; a espessura em pixels físicos deve acompanhar `scale` sem drift de arredondamento
   (ex.: 6 lógicos → 6/8/9 px com tolerância documentada pela regra de arredondamento do Smithay).
   Nenhuma amostra descartada; tolerância calculada antes de ver o resultado.
3. **Nested e multi-output:** `tests/integration/frame_resize_glue.py`, `headless_render_damage.py`,
   `multi_output_damage.py` seguem OK; verificar no nested com `--scale 1.5` que o frame e o painel
   não desalinham (a escala do output).
4. **Regressão 1.0:** todos os harnesses de frame do RUNBOOK seguem verdes em `--scale 1.0` (padrão).
5. Gate: `bash scripts/check.sh` verde; `cargo clippy --workspace --all-targets --features bench -- -D warnings`
   EXIT=0; `cargo check --workspace --no-default-features` EXIT=0.
6. Commit pequeno por etapa validada (regra 10); sem push.

## Juiz visual
Se a escala fracionária mudar o que aparece na tela, **o humano decide** (AGENTS.md §0.9): agente
prepara/deixa pronto, humano testa no tty3 e dá o "ok". **NÃO abrir F1g sem esse ok.**

## Como usar o domicile — guia, NÃO receita
Invariante transferível: a geometria de apresentação é do compositor final e diagnósticos nomeados
(`FSCALE_SURFACE`) para responder por que uma superfície recebeu determinada escala. **Não copiar**
Chromium/Ozone/`cc::SurfaceLayer`/prior art do fork. Fonte executável: Smithay 0.7.0 fixado (citado).

## Fora de escopo
- Backend udev/DRM, dmabuf/presentation-time (F1g), outros protocolos, xdg-activation/IME/ícone.
- Mudança na geometria do 9-slice aprovada (insets, slices, canvas), lock/PAM/greeter, services/apps.
- `protocols/*.xml`, ADR, dependência nova, sudo/VT/push, abertura de F1g sem ok.

## Se encontrar OUTRO problema
Anotar 1–3 linhas em `STATE.md §6` (sintoma, onde, hipótese) e VOLTAR; no máximo 15 min de desvio.

## Limites
- Tempo: 2 h sem medição estável de pixels → PARAR e reportar com dados; sem terceira via por conta própria.
- Dependências: nenhuma.

## Entrega
Relatório curto + `STATE.md §3/§7/§9` + commit local. PARAR antes de F1g.
