# Tarefa F1i+ — text_input/input_method (F1) + xdg_activation (puxado da F7)

> **FECHADA PELO JUIZ VISUAL em 2026-11-22.** Humano operou a nested (`WAYLAND_DISPLAY=wayland-aero-f1i-1795386555 foot`)
> e relatou: *"apareceu apenas compositor does not implement XDG toplevel icon protocol"* (fora de escopo) e,
> questionado sobre o teclado: *"Sim, digitou normal — fechar"*. O compositor (PID 243367) seguiu vivo após o
> teste de ativação. Nenhuma instância foi encerrada pelo agente; quem fecha é o humano.
> Um cartão por sessão. Se o objetivo mudar no meio, PARE e pergunte.
> `xdg_toplevel_icon` JÁ está implementado (foot confirma) e `slave exited… hangup` NÃO é protocolo: NÃO tocar.

## Objetivo (uma frase, verificável)
O compositor anuncia `zwp_text_input_manager_v3`, `zwp_input_method_manager_v2` e `xdg_activation_v1`
e o `foot` conectado ao nested não imprime mais os avisos de "text input … IME disabled" nem de
"xdg activation", sem quebrar a digitação normal nem derrubar o compositor ao ativar uma superfície.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-compositor/src/state.rs` — globals em `AeroState::new` (`rfb.new_global`/`ManagerState::new`) e struct `AeroState`.
- `crates/aeroshell-compositor/src/protocols/seat.rs` — `focus_changed` (`set_data_device_focus`/`set_primary_focus`) e delegates.
- `crates/aeroshell-compositor/src/window_ipc.rs:104-114` — padrão existente de elevação+foco (`space.raise_element`, `keyboard.set_focus`).
- `crates/aeroshell-compositor/Cargo.toml:21-26` e `Cargo.toml:21` — nenhuma dependência nova: os 3 módulos já vêm de `smithay/wayland_frontend`, ativo.

### APIs CONFIRMADAS no Smithay 0.7.0 fixado (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/smithay-0.7.0/`)
- `wayland/text_input/mod.rs:87-106` `TextInputManagerState::new::<D>(&dh)` + `:177` `delegate_text_input_manager!`;
  `text_input/text_input_handle.rs:96-131` `TextInputSeat::text_input()` → `set_focus`/`leave`/`enter` (foco NÃO é automático: a doc de `mod.rs:8` descreve a intenção, o código exige a chamada explícita no `focus_changed`).
- `wayland/input_method/mod.rs:100-113` `InputMethodHandler` (4 métodos obrigatórios: `new_popup`, `dismiss_popup`, `popup_repositioned`, `parent_geometry`), `:115-124` `InputMethodSeat`, `:136-160` `InputMethodManagerState::new::<D, F>(&dh, filter)` (filtro: só clientes de confiança devem ver o global de IME), `:242` `delegate_input_method_manager!`; `PopupSurface` em `input_method/input_method_popup_surface.rs`.
- `wayland/xdg_activation/mod.rs:171-238` `XdgActivationState::new::<D>(&dh)`, `:244-274` `XdgActivationHandler` (`activation_state`, `request_activation`, `token_created` com default), `:108-140` `XdgActivationTokenData` (`client_id`, `serial`, `surface`, `timestamp`), `:286` `delegate_xdg_activation!`.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste-first (globals):** harness versionado que lista globals (cliente `wayland-info` ou cliente de teste) contra o socket do nested e asserta as 3 interfaces presentes. Baseline com o binário atual: `VEREDITO: FALHA` — `zwp_text_input_manager_v3`, `zwp_input_method_manager_v2` e `xdg_activation_v1` AUSENTES. Depois da correção: 3× `VEREDITO: OK` com as 3 presentes. Nenhuma amostra falha descartada.
2. **Texto do foot:** com o nested e `foot`, o stdout/stderr NÃO contém mais `text interface not implemented compositor, IME will be disabled` nem `compositor does not implement xdg activation`. Avisos fora deste cartão (ex.: `hangup`) podem permanecer e devem ser listados.
3. **Integração de foco:** no `focus_changed` do seat, `seat.text_input().set_focus(surface)` + `leave`/`enter` acompanham o foco de teclado; sonda sob `bench` (prefixo novo, e.g. `TI_FOCUS t=… surface=…`) e `strings` do binário sem bench → 0 ocorrências. `commit_string` do cliente chega à superfície focada (o Smithay repassa ao IME; não reimplementar o protocolo).
4. **Ativação:** handler de `xdg_activation` eleva/foca a superfície do token via `space.raise_element` + `keyboard.set_focus`, com prevenção básica de focus-stealing (exigir token com `serial`/`surface`/`client_id` válido e recente; rejeitar token sem associação). Sonda sob `bench` (e.g. `XDG_ACTIVATE … allow/deny`).
5. **Regressão (execução única pós-fix):** nested + headless + DRM (harnesses de frame verdes) + `bash scripts/check.sh` verde + `cargo clippy --workspace --all-targets --features bench -- -D warnings` = 0 + `cargo check --workspace --no-default-features` = 0.
6. **Juiz visual (humano):** `foot` digita normalmente (text_input não pode quebrar o teclado); bell/urgent/ativação não derruba o compositor. Aplicação deixada aberta; quem fecha é o humano (AGENTS.md §0.9). Sem "ok", fica em `STATE.md §7`.
7. `docs/STATE.md` registrado: `xdg_activation` puxado da F7 **por decisão do humano**; `text_input/input_method` = **F1i**; provas, números e não-medidos conforme §3/§7/§9.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Geometria/9-slice; `protocols/*.xml` privados; ADRs; dependências novas (Smithay já provê os módulos).
- `cursor-shape` (cartão separado) e `xdg_toplevel_icon` (já implementado); `slave exited… hangup` (não é protocolo).
- IME real com fcitx/ibus — aqui é só presença do protocolo + digitação normal.
- Abrir próximo cartão sem o "ok" visual do humano nesta sessão.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md §6` (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE. Não investigue desvios por mais de 15 min.

## Limites
- Tempo: até ~2 h sem resultado verificável → pare e descreva o bloqueio.
- Dependências novas: nenhuma. Arquivos protegidos: ver AGENTS.md §6 — nenhum será alterado.
- Regras das duas tentativas e do gate numa única execução (AGENTS.md §0.5) valem aqui.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais.
Ao final: `bash scripts/check.sh` colado, `STATE.md` atualizado e commit local (sem push).
