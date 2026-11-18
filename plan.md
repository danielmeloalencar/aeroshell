# AeroShell — plano (resumo)

**Fonte única de status, números, decisões e próximos passos: [`docs/progress.md`](docs/progress.md).**
Este arquivo é apenas um resumo estável para não divergir do progresso.

Framework pré-alfa de compositores Wayland cuja UI é TSX/JSX em WebViews nativas (WebKitGTK 6.0).
Mecanismo em Rust/Smithay, política em Bun/TypeScript. Ambiente de referência: Ubuntu 26.04,
Rust 1.100.0, Bun 1.4.4, VM sem 3D (funcionamento validado; desempenho só em GPU real).

- **Fase atual**: F0 — spikes go/no-go. S1/S2/S3 concluídos; `aero_frame_v1` v1 gerado, seguro e
  testado; desenho do frame e S4/S6/S7 pendentes.
- **Próximo**: protótipo mínimo do frame com a métrica congelada (configure → commit do frame por
  passo de resize). **Parar e reportar antes de S4/S6/S7 ou Fase 1.**
- **Decisões**: ADRs em [`docs/adr/`](docs/adr/). ADR 0001 (provisório): `related-view` + pool de
  latência; ADR 0002: pins/licenças; ADR 0003: blur apenas via `ext-background-effect-v1`, depois
  do protótipo.
- **Como verificar**: `bash scripts/doctor.sh`, `just check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `bun test`.
- **Fronteiras**: compositor não conhece GTK/WebKit; clientes não conhecem Smithay; só
  `backend/*.rs` distingue nested/real/headless; segredos e tempo-real em Rust.
- **Regra de ambiente**: nunca sobrescrever `WAYLAND_DISPLAY` do host; `WAYLAND_SOCKET` jamais no
  ambiente do compositor (use `AERO_PRIVILEGED_FD`); sem `/etc/pam.d` sem aviso.

Histórico detalhado, hashes, benchmarks e limitações: [`docs/progress.md`](docs/progress.md).
Plano completo (F0–F9, regras e visão): [`docs/plano-mestre.md`](docs/plano-mestre.md) (recuperado
do `Agents.md` histórico) e [`docs/plano-completo-nested.md`](docs/plano-completo-nested.md) (matriz
nested/udev). Guia rápido: [`README.md`](README.md). Convenções: [`AGENTS.md`](AGENTS.md).
