# Tarefa F7-compat-snap — apps snap (Firefox) como cliente de sessão (RASCUNHO — aguarda "ok")

> Cartão de integração/sessão, **não** de protocolo Wayland e **não** "descartável como ambiente/DX".
> Aberto ao fechar a F1: o gate final mostrou `/snap/bin/firefox` com `Permission denied` no socket do nested.
> O problema de Ozone do `/snap/bin/code` (`--ozone-platform=x11` injetado) é **separado** e segue como DX.

## Objetivo (uma frase, verificável)
Descobrir, com dado cru, por que um app snap (Firefox) conecta ao GNOME mas não ao AeroShell, e decidir se a correção é
nome/caminho do socket da sessão ou registro de slot Wayland no snapd — sem inventar API de snap.

## Dado já entregue (2026-11-23) — NÃO prova a hipótese
- `~/u --socket wayland-0` no tty3: `aeroshell-udev-20261123-202747.log` mostra `socket=wayland-0` e morre com
  **`Error: AlreadyInUse`** antes de `Created new socket`. `/run/user/1000/wayland-0` pertence ao **GNOME Shell**
  (PID 3531) da sessão do host — não estava livre.
- O humano então rodou `./u` **sem parâmetros** (socket custom `wayland-aero-tty3`,
  `aeroshell-udev-20261123-202837.log`) e, dentro de um `foot`, `WAYLAND_DISPLAY=wayland-0 firefox`. Isso gerou
  `/home/daniel/Projetos/aeroshell/firefox.log`: WebRender/telemetria sobem **sem `Permission denied`**, mas o
  Firefox estava conectado no **GNOME**, não no AeroShell.
- Conclusão: o snap prova que acessa o socket padrão `wayland-0` da sessão GNOME; **o teste do AeroShell como sessão
  ainda não foi feito**. Manter este cartão com o dado; **não reduzir** para "doc + socket padrão" e **não fazer
  snapd/slot**.

## Plano do experimento de sessão (SÓ com aprovação explícita; NÃO intercalar com F2)
1. **Boot sem GNOME** (ou sair da sessão GNOME) de forma que `wayland-0` esteja livre; confirmar
   `ls -l /run/user/1000/wayland-0` → ausente antes de subir o AeroShell.
2. Subir AeroShell **como sessão** com socket padrão: `~/u --socket wayland-0` (verificar log: `Created new socket
   name=Some("wayland-0")`).
3. Rodar `WAYLAND_DISPLAY=wayland-0 firefox` **sem** forçar sockets custom; capturar stdout/stderr cru.
4. Alternativa (se o boot sem GNOME não for possível): usar **`XDG_RUNTIME_DIR` privado** por sessão,
   sem alterar `/etc` nem o ambiente do host de forma permanente — também exige aprovação e experimento separado.
5. Só depois do dado: decidir reduzir este cartão para doc + socket padrão como sessão, ou manter a investigação.
   `snap connect` / registrar slot no snapd permanece **fora**, com aprovação explícita separada.

## Contexto mínimo — causa levantada (HIPÓTESE, precisa de prova)
- Gate final (2026-11-23, `/tmp/opencode/f1-final-gate/firefox.log`):
  `Failed to connect to Wayland display '/run/user/1000/snap.firefox/wayland-aero-f1-final': Permission denied`.
- snapd só disponibiliza no namespace do snap o socket da **sessão** com nome/caminho padrão. No GNOME isso funciona
  (a sessão já é Wayland e o socket é `wayland-0`); no nested do AeroShell usamos socket **custom**
  (`wayland-aero-f1-final`) e o compositor não é uma sessão — logo snapd não provisiona esse arquivo.
- Isto **não é falha de protocolo**: `wp_presentation`, `linux-dmabuf`, `data-device`, xdg-shell etc. estão provados
  por clientes não-snap nos cartões fechados da F1.
- `docs/STATE.md §6/§7` — Nautilus 50/Mutter ServiceChannel e Ozone do VSCode são itens distintos, fora deste cartão.
- `AGENTS.md §3.8`: mexer em `/etc`, `snap connect`, registrar slot no snapd ou trocar o ambiente do host exige
  aprovação explícita. O agente NÃO troca VT; a operação no tty3 é do humano.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Prova no tty3 como sessão (humano opera; agente apenas instrui):** com o compositor rodando como sessão no tty3
   (`~/u`, socket padrão da sessão) e `DISPLAY`/`WAYLAND_DISPLAY` do host limpos, rodar `firefox` (snap) → o app
   conecta e abre janela, OU sai com erro cru diferente de `Permission denied` em `/run/user/1000/snap.firefox/…`.
   Registrar a saída crua em `/tmp/opencode/f7-snap/tty3-firefox.log`.
2. `snap connections firefox | grep -E 'wayland|x11'` → saída crua mostrando quais interfaces/plugs o snap tem e se
   há slot `wayland` conectado; anexar ao relatório.
3. **Decisão com dado (uma das duas, não as duas às cegas):**
   - (a) como sessão, usar o **nome padrão** `wayland-0` para o socket do compositor e confirmar que o snap conecta;
     ou
   - (b) documentar/registrar o slot Wayland no snapd (somente com aprovação explícita por mexer fora do repo) e
     confirmar a conexão.
4. `bash scripts/check.sh` verde (nenhuma mudança de protocolo deve ser necessária; se houver, PARAR e reportar).
5. Relatório com: qual das opções funcionou, saída crua, e se o mesmo mecanismo cobre `/snap/bin/code` (Ozone é
   separado — não prometer que cobre).

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Reabrir protocolos F1 (dmabuf, presentation, clipboard, xdg-states) — estão provados e fechados.
- Nautilus 50 / `org.gnome.Mutter.ServiceChannel` — cartão/ressalva própria, distinto.
- Ozone do VSCode (`--ozone-platform=x11`) — DX separada, não é provisioning de socket.
- Instalar snaps, alterar `/etc`, `snap connect` ou qualquer comando fora do repositório SEM aprovação explícita.
- Avançar de F2 por causa deste cartão sem decisão do humano.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md §6` (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE. Máximo 15 min de desvio.

## Limites
- Tempo: 2 h sem prover a conexão do snap → PARAR e reportar com a saída crua; sem terceira via por conta própria.
- Dependências novas: nenhuma no repositório; mudanças no host/snapd exigem aprovação.
- Arquivos protegidos: ver `AGENTS.md §6`.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §9) e comitar localmente (sem push).
