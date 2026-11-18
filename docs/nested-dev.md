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

Durante o desenvolvimento:
```bash
LD_PRELOAD=libgtk4-layer-shell.so.0 aeroshell-shell
```
Em produção, garantir a ordem de linkagem na crate `aeroshell-shell` (F2/S2) e documentar o
resultado do spike aqui.

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
