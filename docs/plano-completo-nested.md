AeroShell: plano revisado com modo aninhado (nested)
O que mudou

O Smithay já tem o backend winit (é o que o anvil usa), então o nested não exige tecnologia nova. O que exige é tratá-lo como cidadão de primeira classe, não como gambiarra de desenvolvimento. O plano passa a ter:

Três backends atrás de um trait: winit (nested, padrão em dev), udev (sessão real) e headless (CI e testes). Escolha automática: se existir WAYLAND_DISPLAY ou DISPLAY, usa winit; senão, udev.
aero dev roda nested por padrão. O compositor abre numa janela do compositor atual, com HMR ligado, DevTools do WebKit e socket Wayland próprio (ex.: wayland-aero-0), exportado como WAYLAND_DISPLAY só para os apps lançados lá dentro. Seu desktop real não é tocado.
Paridade de funcionalidades: toda feature precisa funcionar nested, exceto o que for fisicamente impossível, e esses casos são simulados (matriz abaixo).
Serviços e autenticação com mock: nested não pode brigar com o desktop host por nomes D-Bus (ex.: org.freedesktop.Notifications) nem tocar no PAM real.
Abstração Mod: o host captura a tecla Super, então "Mod+Return" resolve para Super na sessão real e para Alt no nested (configurável).
Matriz: o que acontece no nested
Recurso	Nested (winit)	Sessão real (udev)
Janelas, layouts, workspaces, painéis, frames, HMR	Real	Real
Multi-monitor	Simulado: --outputs 2 cria N saídas virtuais, ou N janelas winit	Real (DRM)
Resize da janela host	Vira mudança de modo do output, relayout automático	Hotplug/modo DRM
Escala fracionária	Herdada da escala do host + --scale	Real
Atalhos com Super	Mod = Alt; usa keyboard-shortcuts-inhibit do host se existir	Super
Lock screen (ext-session-lock)	Real, trava só a janela nested	Real
Autenticação	Mock (usuários de teste); PAM real só com --real-auth	PAM real
Greeter	Real, contra um greetd mock embutido	greetd real
Idle (timers, dim, lock)	Real	Real
DPMS off	Simulado (janela fica preta)	DRM ACTIVE
Suspend / lid switch	Simulado (overlay “suspend simulado” + botão de resume)	logind
Troca de VT	N/A	Real
D-Bus (bateria, rede, áudio...)	--mock-services (dados fake controláveis) ou barramento real em modo leitura	Real
Notificações / tray	Barramento de sessão privado (dbus-run-session) para não colidir com o host	Barramento real
XWayland	Opcional	Real
Benchmarks de fps/latência	Não representativos (medir em udev/VM)	Representativos
Árvore: só o que mudou
crates/aeroshell-compositor/src/
├─ cli.rs                        # --backend --size --outputs --scale --mod --mock-services --real-auth --greeter
├─ backend/
│  ├─ mod.rs                     # trait Backend + autodetecção (winit | udev | headless)
│  ├─ winit.rs                   # nested: janela(s) no compositor/X11 atual
│  ├─ udev.rs
│  ├─ headless.rs                # sem GPU visível: CI, screenshots determinísticos
│  └─ simulated.rs               # DPMS, suspend e multi-output virtual (só nested/headless)
crates/aeroshell-auth/src/mock.rs            # usuários e prompts fake; PAM real só com flag
crates/aeroshell-services/src/mock/          # battery, network, audio... controláveis via CLI
packages/cli/src/dev.ts                      # `aero dev` → sobe nested + Bun + HMR
packages/cli/src/dev-controls.ts             # `aero dev set battery 15`, `aero dev suspend`, `aero dev hotplug`
packages/runtime/src/keybindings.ts          # resolve Mod → Super | Alt
scripts/nested.sh                            # fallback: dbus-run-session + env sem o CLI
tests/integration/                           # roda sobre backend headless
docs/nested-dev.md

Comandos de desenvolvimento:

aero dev                          # nested, 1280x720, 1 saída, Mod=Alt, mocks ligados
aero dev --outputs 2 --size 1920x1080 --scale 1.5
aero dev --real-services          # lê D-Bus real do host (somente leitura)
aero dev --greeter                # testa o login contra greetd mock
aero dev --session                # sessão real (TTY); só quando o nested já estiver estável
aero dev set battery 12           # controla serviços mock em tempo real
aero dev suspend | resume | hotplug