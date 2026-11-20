# Tarefa 05 — `progress.md` apenas como histórico

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
Nenhum arquivo ativo manda ler, seguir ou atualizar `docs/progress.md`; ele é apenas arquivo histórico.

## Contexto mínimo (arquivos que importam)
- `AGENTS.md:3,154,262,272` — manda ler, chama de fonte única e pede atualização no fim da tarefa
- `plan.md:3,4,24` — chama `progress.md` de fonte única de status
- `README.md:9-10` — chama `progress.md` de fonte única
- `justfile:35` — mensagem do comando `spike` aponta para `progress.md`
- `docs/plano-mestre.md:208,216-222,331` — plano ATIVO; só trocar as instruções de atualizar `progress.md`
- `docs/STATE.md:5` — já define `progress.md` como arquivo histórico

## Critério de aceite (comandos exatos + resultado esperado)
1. `grep -rn "progress" AGENTS.md plan.md README.md justfile docs/STATE.md` → só menções que dizem
   explicitamente "histórico"/"antigo"; nenhuma manda ler, seguir ou atualizar.
2. `grep -rn "fonte única" AGENTS.md plan.md README.md docs/STATE.md` → aponta para `docs/STATE.md`.
3. `docs/plano-mestre.md` continua sendo o plano ativo, com o texto original preservado; apenas os ponteiros de
   atualização (`progress.md` → `STATE.md`) mudam. NÃO rebaixar este documento a histórico.
4. `bash scripts/check.sh` verde.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Apagar ou reescrever o conteúdo histórico de `docs/progress.md`
- Corrigir dados antigos de `plan.md`/`README.md` ou reclassificar `docs/plano-mestre.md` (é o plano ativo)
- Código, protocolos, ADRs, dependências

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 1 h
- Dependências novas: nenhuma
- Arquivos protegidos: ver AGENTS.md §6 — `AGENTS.md` alterado com aprovação explícita do usuário nesta tarefa

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §6, §9) e comitar localmente (sem push).
