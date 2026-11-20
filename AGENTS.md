# AGENTS.md — Regras para o agente (AeroShell)
 
Leia este arquivo e `docs/progress.md` no INÍCIO de toda sessão. Estas regras têm
prioridade sobre qualquer instrução vaga em conversa. Se uma instrução do usuário
conflitar com elas, avise o conflito e peça confirmação antes de agir.

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
 
## 1. O que é o projeto
 
AeroShell é um framework para criar compositores Wayland completos cuja parte visual
(painéis, widgets, frames de janela, lock, login) é escrita em TSX e renderizada em
WebKitGTK. Compositor em Rust + Smithay; política em Bun/TypeScript. Roda nested
(janela dentro do compositor atual) para desenvolvimento e em sessão real (TTY).
 
Plano completo: `docs/architecture.md`. Status atual: `docs/progress.md` (fonte única).
Decisões: `docs/adr/`. Modelo de ameaça: `docs/security-model.md`.
 
## 2. Decisões congeladas (NÃO reabra sem aprovação explícita)
 
- WPE WebKit está AUSENTE no Ubuntu 26.04. Não proponha WPE como padrão.
- WebView = WebKitGTK 6.0 como CLIENTE Wayland (GTK4 + gtk4-layer-shell).
  O compositor não conhece GTK/WebKit; shell/lock/greeter não conhecem Smithay.
- Frames de janela: uma WebView por janela com `related-view` de uma view-âncora
  oculta (sempre viva) + pool pequeno pré-aquecido. Atlas = Plano B com gatilho em ADR.
- Protocolo privado `aero_frame_v1`: EXPERIMENTAL e congelado para a Fase 0.
  Mudanças no XML exigem aprovação.
- Blur: somente `ext-background-effect-v1`. Não recrie blur próprio.
- Lock e greeter: processos próprios e autônomos (sem Bun, sem dev server em produção).
  A máquina de estados do lock vive no compositor. Fail-secure SEMPRE.
- Mecanismo em Rust, política em TS. Tempo-real (grabs, animação, idle, lock) em Rust.
- A senha NUNCA passa pelo Bun, pelo SDK geral, por logs ou métricas.
- Backends: `winit` (nested), `udev` (real), `headless` (testes). Só `backend/*.rs`
  conhece a diferença. Toda feature precisa funcionar no nested ou ter simulação documentada.
- Rust via `rustup` com versão fixada em `rust-toolchain.toml` (versão mínima necessária
  ao Smithay, não a mais nova instalada). Build de pacote no Debian 13 (trixie).
## 3. PROIBIDO (nunca faça, mesmo que pareça mais rápido)
 
1. Avançar de fase sem aprovação. Ao terminar uma tarefa, PARE e reporte.
2. Afirmar que um teste passou sem tê-lo executado. Dizer "não executei" é aceitável.
3. Descartar, ignorar ou "corrigir em silêncio" uma medição ou teste que falhou.
4. Resolver um erro desligando a checagem (`--release` para sumir um abort, `#[allow]`,
   `unsafe` para calar o compilador, `.unwrap()` para passar, comentar o teste).
5. Usar PID ou variável de ambiente para decidir privilégio de cliente Wayland.
6. Inventar APIs de Smithay, GTK, WebKitGTK, gtk4-layer-shell, PAM, greetd ou D-Bus.
   Leia o código-fonte/doc da VERSÃO FIXADA (ex.: `.gir` instalado, crate no `~/.cargo`).
7. Commitar `Box::leak`, ponteiros brutos, `from_raw_fd` em fd com dono, ou `unsafe`
   sem comentário `// SAFETY:` que justifique.
8. Alterar o ambiente do host: sem `sudo`, sem tocar em `/etc` (inclui `/etc/pam.d` e
   display manager), sem instalar pacotes do sistema, sem sobrescrever `WAYLAND_DISPLAY`
   globalmente, sem reivindicar nomes D-Bus do host. Tudo fora do repositório: pergunte.
9. Fazer `git push --force`, reescrever histórico, apagar branches ou arquivos fora do
   escopo da tarefa.
10. Adicionar dependência (crate/npm/apt) sem listar: nome, versão, licença, motivo e
    alternativa. Aguarde aprovação.
11. Tocar em segurança (lock, PAM, greeter, filtro de globais) sem testes de segurança.
12. Trocar de abordagem uma terceira vez para o mesmo problema (ver regra 4.5).
## 4. Método obrigatório
 
### 4.1 Investigação de problema
1. Enuncie a hipótese em uma frase ANTES de mexer em código.
2. Reproduza o problema e mostre a saída crua. Se não reproduziu, diga "não reproduzi".
3. Corrija UMA coisa por vez.
4. Prove que funcionou com o mesmo comando do passo 2.
5. REGRA DAS DUAS TENTATIVAS: se a mesma abordagem falhar 2 vezes, PARE. Escreva o que
   tentou, o que descobriu e peça decisão. Não tente uma terceira variação por conta própria.
### 4.2 Implementação
- Teste primeiro: escreva o teste que falha, mostre-o falhando, implemente, mostre passando.
- Mudanças pequenas e compiláveis a cada passo. Um commit por unidade lógica.
- Antes de criar arquivos, confira a árvore em `docs/architecture.md`. Crie só o que a
  fase atual exige. Não crie diretórios vazios "para o futuro".
### 4.3 Evidência
- Toda afirmação técnica cita a fonte: `arquivo:linha`, saída de comando, ou documentação
  instalada. Distinga SEMPRE: "medi", "li na documentação", "estou supondo".
- Benchmarks: 3 repetições, aguardar estabilização por polling (variação < 2% em 3 leituras),
  PSS por processo (não só RSS nem só a soma), release build, informar se é VM sem 3D.
- Sanidade: confira se os números são fisicamente plausíveis (mais carga não usa menos
  memória). Se algo parece estranho, investigue antes de concluir.
- Nenhuma amostra pode ser descartada em silêncio: repita e registre.
- VirtualBox sem 3D: valide FUNCIONAMENTO; desempenho só vale em GPU real.
### 4.4 Segurança
- Fail-secure no lock: se shell, runtime Bun ou cliente de lock morrerem, a sessão
  permanece trancada. Nunca destrancar por erro, timeout ou crash.
- Segredos (senhas, tokens) nunca em logs, métricas, mensagens de erro ou IPC geral.
  Buffers de senha com `zeroize`.
- WebViews só carregam `aero://` (produção) ou o dev server local (só em dev).
  Sem navegação externa. Permissões por superfície.
- Teste PAM real e greetd real SOMENTE em VM. Use backend mock no nested.
## 5. Quando PERGUNTAR antes de agir
 
Pare e peça aprovação quando:
- a tarefa exigir mudar arquitetura, protocolo (`protocols/*.xml`), ADR aceito ou fase;
- precisar de dependência nova, `sudo`, pacote do sistema ou alteração fora do repositório;
- um requisito conflitar com a realidade técnica (apresente dados e alternativas);
- houver dúvida sobre a API de uma biblioteca (crie um spike, não chute);
- a correção exigir apagar ou reescrever código existente em grande escala.
## 6. Arquivos protegidos (alteração só com aprovação explícita)
 
- `protocols/*.xml`
- `docs/adr/*` (ADR aceito não é editado; escreva um novo que o substitua)
- `docs/security-model.md`, `packaging/pam.d/*`, `packaging/greetd/*`
- `rust-toolchain.toml`, `Cargo.lock` (exceto por mudança aprovada de dependência)
- `AGENTS.md` (este arquivo)
## 7. Qualidade (rode antes de dizer "concluído")
 
```
bash scripts/check.sh   # doctor + fmt --check + clippy -D warnings + cargo test + bun test
```

Fonte única de verificação: `scripts/check.sh` (versionado). **Não usar `just`**: não está
instalado neste ambiente e instalar fora do repositório exige aprovação. CI, agentes e
desenvolvimento rodam exatamente este script. Se ele falhar, a tarefa NÃO está concluída.
Testes que exigem display (nested) ficam em scripts separados e devem ser rodados manualmente
e reportados.
 
## 8. Formato de relatório (toda resposta de fim de tarefa)
 
1. O que foi feito (3 a 6 linhas).
2. Para cada verificação: comando executado → saída crua → interpretação em 1–2 frases.
3. O que NÃO foi feito ou não foi testado, e por quê.
4. Riscos e incertezas ("estou supondo que…").
5. Liste 3 maneiras pelas quais sua conclusão pode estar errada e o que checou para cada.
6. Próximo passo proposto. NÃO o execute. Aguarde aprovação.
Atualize `docs/progress.md` ao fim de cada tarefa: decisões, pendências, números medidos
(com a ressalva de ambiente) e o que está "não medido".
 
## 9. Checklist de fim de tarefa
 
- [ ] `bash scripts/check.sh` verde (saída colada no relatório)
- [ ] Testes novos escritos primeiro e vistos falhando
- [ ] Nenhum `unsafe`/`unwrap`/`allow` novo sem justificativa
- [ ] Nenhum arquivo protegido alterado sem aprovação
- [ ] Nenhuma dependência nova sem aprovação
- [ ] `docs/progress.md` atualizado
- [ ] Relatório no formato da seção 8, com o que NÃO foi testado
- [ ] Parou. Não iniciou a próxima fase.
## 10. Erros já cometidos neste projeto (não repita)
 
- Diagnosticar causa (ex.: margem horizontal) sem reproduzir; era uma scrollbar por overflow.
- Concluir "pool obrigatório" a partir de números inconsistentes (PSS com N=20 menor que N=1).
- Testar `related-view` pelo mecanismo errado (WebContext) antes de ler a documentação.
- Ler `WAYLAND_SOCKET` no próprio compositor (nome reservado a bibliotecas cliente) e
  contornar o abort com tentativas sucessivas em vez de investigar a posse do fd.
- Manter duas rotas para a mesma funcionalidade (ex.: blur próprio e padrão).
- Declarar teste "coberto" quando só existia teste unitário sem cliente real.

Protocolo de sessão (obrigatório)
 