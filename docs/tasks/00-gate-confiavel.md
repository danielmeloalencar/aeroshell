# Tarefa 00 — Gate confiável

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
`bash scripts/check.sh` passa verde numa única execução, três vezes seguidas, com a máquina ociosa.

## Contexto mínimo (arquivos que importam)
- `scripts/check.sh` — gate; NÃO editar para passar (AGENTS.md §0.5)
- `tests/integration/s1_nested.test.ts` — sobe o nested; hoje mostra só "esperava 0, recebeu 1", sem stderr
- `tests/integration/*.py` — harnesses com `pkill -x aeroshell-compositor` (não casa: `comm` truncado a 15 caracteres)
- `docs/STATE.md` §6 — flake do S1 já registrado (falhou com exit 1, isolado passou)

## Critério de aceite (comandos exatos + resultado esperado)
1. `bash scripts/check.sh` → `OK: scripts/check.sh passou` em 3 execuções seguidas, sem editar o script.
2. `bun test tests/integration/s1_nested.test.ts` com falha induzida → o stderr do compositor aparece na saída;
   o compositor é encerrado pelo PID que o próprio teste iniciou (no `finally`).
3. `grep -rn "pkill -x" tests/` → nenhum nome > 15 caracteres; harnesses rastreiam PIDs próprios.
4. Nenhuma amostra de teste descartada em silêncio; falhas registradas em `docs/STATE.md` §6.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- window_id, decoração por janela, resize do frame, fundo branco no gap
- latência do resize, pool de WebViews, painel React, SDK

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem gate estável → pare e descreva o bloqueio
- Dependências novas: nenhuma
- Arquivos protegidos: ver AGENTS.md §6

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §9) e comitar localmente (sem push).
