# AGENTS.md — Regras para o agente (AeroShell)
 
Leia este arquivo, `docs/STATE.md`, `docs/RUNBOOK.md` e o cartão da tarefa no INÍCIO de toda sessão.
`docs/progress.md` é apenas arquivo histórico: não ler para decidir, não atualizar. Estas regras têm
prioridade sobre qualquer instrução vaga em conversa. Se uma instrução do usuário
conflitar com elas, avise o conflito e peça confirmação antes de agir.

## 0. Como você trabalha neste projeto (método obrigatório)

Você é um engenheiro que trabalha por ciclos curtos, verificáveis e rastreáveis. Você não tem memória entre sessões:
o repositório é sua memória. Se não está escrito em docs/STATE.md, não aconteceu.

### 0.1 Ao começar uma sessão
1. Leia AGENTS.md, docs/STATE.md, docs/RUNBOOK.md e o cartão da tarefa (docs/tasks/).
2. Rode `git branch --show-current`, `git status --short`, `git log --oneline -5`.
   Mudanças que você não fez: descreva e pergunte; nunca descarte.
3. Rode `bash scripts/check.sh`. Se falhar antes de você mexer em algo, essa é sua primeira tarefa (gate confiável).
4. Compare STATE.md com a realidade (git log, testes). Se divergirem, corrija o STATE.md e avise.
5. Responda em 5 linhas: tarefa, critério de aceite, fora de escopo, comando de validação, incoerências achadas. Espere meu "ok".

### 0.2 Tarefa sem cartão = você cria o cartão
Se eu pedir algo sem cartão em docs/tasks/, NÃO comece. Escreva um rascunho a partir de docs/tasks/TEMPLATE.md:
objetivo em uma frase verificável, contexto mínimo (arquivos), critério de aceite com comandos exatos e saída esperada,
fora de escopo, limites (tempo, dependências), e mostre para mim. Se o pedido for vago, faça NO MÁXIMO uma pergunta
(a que mais muda o resultado) e proponha o padrão que você escolheria. Só execute depois da minha confirmação.
Se o pedido for grande, divida em cartões pequenos (cada um entregável e testável em até ~2 horas).

### 0.3 O ciclo de trabalho (sempre nesta ordem)
1. ENTENDER: leia o código e os logs relevantes; cite arquivo:linha.
2. HIPÓTESE: uma frase ("o sintoma X acontece porque Y"). Diga o que você leu e o que está supondo.
3. REPRODUZIR: faça o problema aparecer e mostre a saída crua. Não reproduziu? Diga "não reproduzi" e pare;
   não corrija no escuro. Se o ambiente impede (VM sem 3D, sem display), diga e proponha outra forma.
4. TESTE PRIMEIRO: escreva o teste que falha, mostre-o falhando. Para bugs visuais, escreva a medição objetiva
   (ex.: "frame acompanha o conteúdo em ≤300 ms") e peça meu teste manual como juiz final.
5. CORREÇÃO MÍNIMA: uma mudança por vez, a menor possível. Rode o teste e os testes vizinhos a cada passo.
6. PROVAR: rode o mesmo comando do passo 3/4 e mostre o antes e o depois.
7. FECHAR: gate completo, atualizar STATE.md, commit local, relatório.
Antes de cada ação relevante, uma linha: "isto serve ao critério X porque Y". Se não conseguir escrever, não faça.

### 0.4 Foco (você tende a se perder aqui)
- Uma tarefa por sessão. O critério de aceite do cartão define "pronto".
- Problema fora da tarefa: anote 1 a 3 linhas em STATE.md §6 (sintoma, onde, hipótese) e VOLTE. Só investigue se
  bloquear o critério de aceite, e por no máximo 15 minutos.
- Não troque de objetivo sozinho. Se acha que o objetivo mudou, pare e pergunte citando o motivo.
- Regra das duas tentativas: se a mesma abordagem falhar duas vezes, pare, escreva o que tentou e o que descobriu, e peça decisão.
- Quando eu relatar algo que vi na tela, trate como dado, não como opinião. Pode contradizer seu teste automatizado:
  o teste provavelmente não mede o que eu vi. Registre a lacuna e crie a medição que faltava.

### 0.5 Verdade e atalhos (nunca)
- Só afirme "passou" se executou o comando nesta sessão e viu a saída. Diga "não executei" quando for o caso.
- O gate verde significa TUDO verde numa única execução. Proibido: pular etapas do check.sh (ex.: rodar sem fmt),
  editar o gate para passar, reexecutar até passar, ou chamar de "flake" uma falha que você não investigou.
  Teste instável: corrija ou tire do gate com justificativa em STATE.md; nunca esconda.
- Não desligue verificações para sumir um erro (release, allow, unwrap, comentar teste).
- Não descarte nem "limpe" amostras de medição. Repita e registre.
- Números precisam passar no teste de plausibilidade (mais carga não usa menos memória; p95 ≥ média). Se algo parece estranho,
  investigue antes de relatar.
- Comandos que matam processos: use apenas PIDs que você iniciou. `pkill -x` com nome > 15 caracteres não casa.
- Ambiente é VM sem 3D: valide funcionamento; desempenho só em GPU real. Diga isso ao relatar tempo ou RAM.
- Não invente APIs: leia o código-fonte/doc da versão fixada e cite a fonte.

### 0.6 Higiene de repositório
- Mudança experimental ou quebrada: crie branch `wip/<nome>` e comite lá com mensagem "WIP (quebrado): <sintoma>".
  O master só recebe trabalho que passa no gate. Nunca deixe o master quebrado nem trabalho importante só na árvore de trabalho.
- Commits pequenos, uma unidade lógica, mensagem que diz o porquê. Sem push (eu empurro).
- Arquivos protegidos e dependências: siga AGENTS.md §3, §5 e §6.
- Debug temporário (eprintln!, variáveis de ambiente de teste) é removido antes do commit ou fica sob `cfg(feature = "bench")`.

### 0.7 Ao terminar (mesmo se a tarefa não terminou)
1. `bash scripts/check.sh` completo; cole o resultado final.
2. Atualize docs/STATE.md: §3 feito (com a prova), §4 em andamento, §5 próximas, §6 dívidas/descobertas,
   §7 não medido, §9 handoff (o que fazia, comando exato para reproduzir o estado, próximo passo imediato).
3. Comite localmente.
4. Autorrevisão: releia seu próprio relatório como um revisor cético. Procure números inconsistentes entre si, afirmações
   sem evidência, e itens do critério de aceite que você não executou. Liste 3 maneiras pelas quais sua conclusão pode estar errada
   e o que você checou para cada.
5. Relatório curto, neste formato:
   - Resultado: funciona / parcial / quebrado (uma linha)
   - Como eu testo à mão (comandos exatos e o que devo ver)
   - Confirmação visual do humano: OK / pendente (aplicação deixada aberta)
   - Provas (comando → saída crua → interpretação em 1–2 frases)
   - NÃO executado / NÃO medido
   - Descobertas laterais (já anotadas em STATE.md §6)
   - Próximo passo proposto (não o execute)

### 0.8 Quando perguntar antes de agir
Mudança de arquitetura, protocolo (protocols/*.xml), segurança (lock, PAM, filtro de globais), ADR aceito, dependência nova
fora do workspace, comandos fora do repositório (sudo, /etc, apt), apagar ou reescrever código em grande escala.
Para todo o resto, escolha o padrão recomendado, avise no relatório ("usei X; reverta se discordar") e continue.
Agrupe dúvidas em uma única pergunta, com sua recomendação.

### 0.9 Validação visual: o humano é o juiz final
- Todo critério que depende do que aparece na tela — clique/arrasto de janela, presença e largura do painel,
  formato/posição do cursor, transparência, z-order, ausência de barra dupla, decoração acompanhando o resize —
  NÃO é fechado por teste automatizado nem pela sua própria observação. Teste automatizado é pré-filtro; a sentença
  final é o humano.
- Para cada mudança visual, nesta ordem:
  1. execute o comando de execução/manual do RUNBOOK e deixe a aplicação **aberta**;
  2. diga exatamente o que o humano deve olhar e o que fazer (ex.: "arraste a barra da janela 1 e confirme que só
     ela se move; veja se a barra inferior preenche a largura toda");
  3. peça explicitamente o "ok" do humano;
  4. **não encerre o processo**: quem fecha a aplicação é o humano, depois de testar. Mate apenas PIDs que você
     iniciou e somente se o humano pedir.
- Sem o "ok" do humano nesta sessão, o item fica em `docs/STATE.md` §7 como **não validado** e a tarefa NÃO é
  concluída. Nunca escreva "validado visualmente" com base só em log, screenshot seu ou teste headless.
- Continue medindo o que é mensurável (geometria, insets, contagem de cliques, tempo ≤300 ms) e registre as medições
  como prova complementar; elas não substituem o "vi na tela" do humano. Se o teste automatizado contradizer o relato
  do humano, o relato prevalece: registre a lacuna da medição e crie a medição que faltava (§0.4).

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
 
Plano mestre (ativo, visão e fases): `docs/plano-mestre.md`. Arquitetura: `docs/architecture.md`.
Status atual: `docs/STATE.md` (fonte única). `docs/progress.md` é histórico.
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
Atualize `docs/STATE.md` ao fim de cada tarefa: decisões, pendências, números medidos
(com a ressalva de ambiente) e o que está "não medido". `docs/progress.md` NÃO é atualizado.
 
## 9. Checklist de fim de tarefa
 
- [ ] `bash scripts/check.sh` verde (saída colada no relatório)
- [ ] Testes novos escritos primeiro e vistos falhando
- [ ] Nenhum `unsafe`/`unwrap`/`allow` novo sem justificativa
- [ ] Nenhum arquivo protegido alterado sem aprovação
- [ ] Nenhuma dependência nova sem aprovação
- [ ] `docs/STATE.md` atualizado (`docs/progress.md` é histórico, não é atualizado)
- [ ] Confirmação visual do humano obtida quando o critério é visual; o agente não fechou a aplicação
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
 