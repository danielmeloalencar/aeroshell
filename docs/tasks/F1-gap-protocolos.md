# Tarefa F1-gap-protocolos — cartão-mãe (RASCUNHO — aguarda "ok")

> Cartão-mãe da F1. Cada item é executado por **um subcartão por sessão**, com cartão próprio,
> teste de cliente real, gate verde e commit pequeno (plano-mestre §6, regra 10).
> Origem: gap analysis de 2026-11-22 registrada em `docs/STATE.md §3` (feitos) e `§7` (faltando),
> a partir de `docs/plano-mestre.md:176` (F1: 15 itens).

## Objetivo (uma frase, verificável)
Fechar os 9 itens da F1 classificados como `~` ou `❌` no mapa, um subcartão por vez, com a prova
de cliente real e o gate verdes, sem reabrir o bug do conteúdo JÁ fechado na F1d3.

## Mapa congelado (2026-11-22)
| Item F1 §5 | Class. | Subcartão |
|---|---|---|
| DRM | ✅ | — (F1d1/d2/d3 fechadas) |
| multi-monitor | ✅ | — (F1c fechada) |
| damage | ✅ | — (F1b/F1c) |
| xdg-shell | ✅ | — |
| decoration | ✅ | — |
| layer-shell | ✅ | — |
| subsurfaces | ✅ | — (sem teste dedicado de cliente; ressalva) |
| popups | ~ | `F1-popups-render` |
| data-device/primary-selection | ✅/❌ | `F1-primary-selection` |
| viewporter | ❌ | `F1-viewporter` |
| fractional-scale | ❌ | `F1-fractional-scale` |
| dmabuf | ❌ | `F1-dmabuf` |
| presentation-time | ❌ | `F1-presentation-time` |
| cursor-shape | ❌ | `F1-cursor-shape` (**primeiro**) |
| text-input/input-method | ❌ | `F1-text-input-im` |

Fora da lista F1 §5, mas presentes no rascunho `05-compatibilidade-protocolos.md` (7 avisos do `foot`):
xdg-activation, xdg-toplevel-icon e urgência de toplevel — só viram cartões se o objetivo for
"zero avisos do foot" e o humano autorizar.

## Ordem proposta dos subcartões (NÃO é autorização automática)
1. **F1-cursor-shape** — 1 aviso some; pré-requisitos já conferidos na fonte fixada.
2. **F1-primary-selection** — reusa `SelectionHandler`/foco existentes (`seat.rs:53-66`).
3. **F1-popups-render** — fecha o `~`: popup na cena + `grab()`.
4. **F1-fractional-scale** — toca a escala do 9-slice (`render/scene.rs:40`); exige juiz visual.
5. **F1-viewporter** — independente do anterior no Smithay 0.7.0 (confirmado).
6. **F1-text-input-im** — prova exige cliente IME real.
7. **F1-presentation-time** — ligar feedback ao `queue_frame(())` do udev (dívida §6).
8. **F1-dmabuf** — maior risco; em VM sem 3D vale funcionamento, não desempenho.

Nenhum desses módulos depende de outro no Smithay fixado; a ordem prioriza baixo risco + valor visível.

## Regras comuns a todos os subcartões
- **Abrir subcartão e obter "ok" antes de executar** (AGENTS.md §0.2/§3.1). Este cartão-mãe não autoriza nenhuma implementação por si só.
- Teste primeiro: reproduzir a ausência/sintoma com cliente real e colar a saída crua antes da correção.
- Gate em cada etapa validada: `bash scripts/check.sh`, `cargo clippy --workspace --all-targets -- -D warnings`
  (também com `--features bench` quando houver instrumentação) e `cargo check --workspace --no-default-features`.
- Commit pequeno por etapa validada, com o gate que a autorizou (regra 10). Sem push.
- Critério visual: humano é o juiz final (AGENTS.md §0.9); agente não abre/fecha sessão no tty3 e não troca VT.
- Sem `sudo`, sem tocar em `/etc`, sem push de "concluído" sem ok do humano.
- Sem mudança em `protocols/*.xml`, ADR aceito ou dependência nova sem aprovação explícita (AGENTS.md §3.5/§3.10).

## Fora de escopo (NÃO fazer em nenhum subcartão deste conjunto)
- Reabrir o bug do conteúdo do `foot`/painel no udev — F1d3 está FECHADA pelo juiz ("ok. Aprovado").
- Reabrir 02c/02d/02e/02f/02n/02q ou avançar de fase.
- Lock, PAM, greeter (F5/F6), services/apps (F4).
- Otimizações de desempenho medidas em VM sem 3D (não são conclusivas).

## Entrega
Cada subcartão termina com: o que funciona, como testar à mão, o que NÃO foi testado, gate colado,
`STATE.md` atualizado (§3/§7/§9) e commit local. O fechamento deste cartão-mãe só quando os 9 itens
estiverem fechados e o humano autorizar.
