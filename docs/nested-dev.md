# Desenvolvimento aninhado (nested)

O compositor roda como janela no compositor host. **Nunca** sobrescreva `WAYLAND_DISPLAY` no
ambiente do host: o nome do socket do AeroShell é passado apenas no `env` dos processos filhos
(`--command`, shell, lock, greeter).

## Backends
- `winit`: padrão quando `WAYLAND_DISPLAY` ou `DISPLAY` existem.
- `headless`: `aeroshell-compositor --backend headless` (CI/testes, sem renderização).
- `udev`: sessão real; F1.

## gtk4-layer-shell: ordem de carregamento
A biblioteca **deve** ser carregada antes de `libwayland-client`. Caso contrário a janela vira
toplevel comum, sem erro claro, e `set_anchor`/`set_exclusive_zone` não funcionam.

Durante o desenvolvimento (também usado quando os headers foram instalados em prefixo local):
```bash
LD_PRELOAD=libgtk4-layer-shell.so.0 aeroshell-shell
```

**Resultado do S2 (2026-11-18)**: com `gtk4-layer-shell 0.8.0` (crate Rust) contra a lib C **1.3.0**
(instalada em aptroot local), **não foi possível reproduzir** a falha silenciosa mesmo sem
`LD_PRELOAD` — o compositor recebeu `zwlr_layer_surface_v1` e `exclusive_zone = Exclusive(48)` nos
dois casos. `LD_DEBUG=libs` mostrou que, inclusive com `LD_PRELOAD`, o `calling init` de
`libwayland-client.so.0` vem antes do de `libgtk4-layer-shell.so.0`, e ainda assim funcionou.

**PENDENTE**: o alvo Debian trixie usa **1.0.x**, que ainda **não foi testado**. A defesa
`LD_PRELOAD=libgtk4-layer-shell.so.0` permanece obrigatória em `aero dev`/scripts até que o teste
com 1.0.x seja feito. Não remover esta defesa com base no resultado da 1.3.0.

## Matriz real / simulado
| Recurso | Sessão real | Nested/headless |
|---|---|---|
| Backend de saída | udev/DRM | winit / headless |
| Multi-output | DRM | `--outputs N` (modos virtuais) |
| DPMS/suspend/lid | DRM/logind | simulado (F5) |
| Auth | PAM real (`--real-auth`) | mock (padrão) |
| D-Bus | sessão do host | `dbus-run-session` privado |
| Troca de VT | real | impossível (documentado) |

## Desempenho
VirtualBox sem 3D serve apenas para validar funcionamento. FPS/RAM/latência devem ser medidos
em GPU real; números do VM não são representativos.
