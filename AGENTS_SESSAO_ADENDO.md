# ADENDO para o AGENTS.md — Protocolo de sessão (cole no AGENTS.md como nova seção 0)
 
## 0. Protocolo de sessão (obrigatório)
 
### Início (antes de qualquer edição)
1. Leia, nesta ordem: este arquivo, `docs/STATE.md`, `docs/RUNBOOK.md` e o cartão da tarefa em `docs/tasks/`.
2. Rode `git status --short` e `git log --oneline -5`. Se houver mudanças não commitadas que você não fez,
   NÃO as descarte: descreva-as e pergunte.
3. Rode `bash scripts/check.sh`. Se falhar ANTES de você mexer em algo, reporte e pare (o estado de partida está quebrado).
4. Repita em 3 linhas: qual é a tarefa, qual o critério de aceite, o que está fora de escopo. Se não houver cartão, peça um.
### Durante: foco
- UMA tarefa por sessão. O critério de aceite do cartão define "pronto".
- Ao achar um problema FORA da tarefa: anote 1 a 3 linhas em `docs/STATE.md` §6 e VOLTE à tarefa.
  Só investigue o desvio se ele bloquear o critério de aceite, e no máximo 15 minutos.
- Não troque de objetivo por conta própria (ex.: de "fechar o frame certo" para "ajustar latência").
  Se acha que o objetivo mudou, PARE e pergunte, citando o motivo.
- Antes de cada ação, uma linha: "isto serve ao critério X porque Y". Se não conseguir escrever, não faça.
- Nunca rode comandos que matam processos por nome genérico; `pkill -x` com mais de 15 caracteres não casa.
  Use PIDs que você mesmo iniciou.
### Fim (obrigatório, mesmo que a tarefa não tenha terminado)
1. Rode `bash scripts/check.sh` e cole o resultado.
2. Atualize `docs/STATE.md`: §3 (feito, com a prova), §4 (em andamento), §5 (próximas), §6 (dívidas/descobertas),
   §7 (não medido), §9 (handoff: o que fazia, comando exato do estado atual, próximo passo imediato).
3. Comite localmente com mensagem descritiva. Sem push.
4. Relatório curto: o que funciona, como testar à mão, o que quebrou, o que NÃO foi testado.
### Regra de ouro
Se outro modelo, sem nenhuma memória desta conversa, não conseguir retomar o trabalho lendo só o repositório,
a sessão NÃO terminou: complete o `STATE.md`.
 
---
 
# PROMPT DE BOOTSTRAP (cole como primeira mensagem em qualquer modelo/harness novo)
 
```text
Você está retomando o projeto AeroShell. Você não tem memória de conversas anteriores; o repositório é a fonte de verdade.
1. Leia AGENTS.md, docs/STATE.md e docs/RUNBOOK.md. Depois leia o cartão docs/tasks/<NN>-*.md (tarefa atual: <NN>).
2. Siga o "Protocolo de sessão" do AGENTS.md (início, foco, fim).
3. Antes de editar, responda em 5 linhas: estado atual do projeto, a tarefa e seu critério de aceite, o que está fora de escopo,
   o comando que vai usar para validar, e qualquer incoerência que você notou entre STATE.md e o código (git log/git status).
4. Aguarde meu "ok" para começar.
```
 