# Tarefa F0 — fechamento formal (cartão 0)

> Cartão de rastreabilidade da sessão de fechamento da F0, aprovada pelo humano em 2026-11-21
> (opção 2, escopo restrito). Docs-only: nenhum código de risco.

## Objetivo (uma frase, verificável)
F0 passa a ter fechamento formal verificável: relatório go/no-go com números já medidos, ratificação
registrada dos ADRs, deferimento explícito de S4/S5 e marcador `Fase atual: F1` no `STATE.md`.

## Contexto mínimo
- `docs/STATE.md` — fonte única da verdade; trecho "Fase atual" e handoff (§9)
- `docs/progress.md:26-111,174-197` — números brutos já medidos dos spikes (histórico)
- `docs/adr/0001-go-no-go.md`, `docs/adr/0006-frame-resize-composicao-elastica.md` — só linha de status
- `docs/plano-mestre.md:222-226` — decisão humana de fechar F0 e autorizar F1

## Critério de aceite (comandos exatos + resultado esperado)
1. `docs/f0-relatorio-go-no-go.md` existe e contém quadro S1–S5 com GO/NO-GO por spike e números citados
   com fonte; S2 = NO-GO no caminho WPE → GO WebKitGTK-cliente + canvas fixo; frame spike GO no juiz.
2. Deferimentos registrados: S4 → F2/F9 e S5 (PAM/greetd) → F5/F6, com justificativa.
3. `git diff -- docs/adr/0001-go-no-go.md docs/adr/0006-frame-resize-composicao-elastica.md` →
   **somente** a linha de status de cada ADR; nenhum conteúdo técnico alterado; diff mostrado ao humano.
4. `grep -n "Fase atual: F1" docs/STATE.md` → encontra o marcador; "modo: SPIKE" removido.
5. `frame_cold_start.py` e `frame_resize_sync.py` permanecem como baseline-falha em `STATE.md` §7
   (não são gates; 02p/02q fechadas pelo juiz).
6. `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0).
7. Commit local `docs(f0): relatório go/no-go e fechamento formal — F0 OK`. Sem push.

## Fora de escopo (NÃO faça)
- Alterar conteúdo técnico de ADRs ou qualquer outro arquivo protegido.
- Executar spikes, instalar apt, mexer em `Cargo.toml`, protocolos, código Rust/TS.
- Reabrir 02p/02q pela divergência dos harnesses; re-litigar resize/cold-start.
- Iniciar qualquer cartão F1 nesta sessão.

## Limites
- Tempo: uma sessão.
- Dependências novas: nenhuma.
- Arquivos protegidos: só as linhas de status de 0001 e 0006 (autorização explícita do humano).

## Entrega
Relatório curto no formato de `AGENTS.md` §8, com `git diff` dos ADRs colado e gate verde.
