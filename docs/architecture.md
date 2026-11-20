# Arquitetura AeroShell

## Fronteiras
- `aeroshell-compositor` (Rust/Smithay) é dono do estado e do input. Não conhece GTK/WebKit.
- `aeroshell-shell`, `aeroshell-lock`, `aeroshell-greeter` são clientes Wayland e clientes do
  IPC; não conhecem Smithay.
- Somente `backend/*.rs` conhece nested (`winit`), real (`udev`) e `headless`.
- Política (layouts, regras, workspaces, idle) vive no runtime Bun; tempo-real e segredos em Rust.

## Superfícies e reset visual
Todas as superfícies carregam `aero://shell-reset.css` (arquivo único, servido pelo esquema
registrado no `WebContext` do host). Isso garante caixa consistente, ausência de overlay
scrollbar, `user-select:none` e cursor padrão antes de qualquer código de componente rodar.
Componentes **podem sobrescrever explicitamente** qualquer regra — ver
`docs/sdk-reference/shell-reset.md`. Em produção só `aero://` é carregado; o dev server local
do Bun é exclusivo de desenvolvimento.

## Camadas
`desktop < [frame + cliente por janela] < panels/overlay < lock`. Layer surfaces passam pelo
`LayerMap` do Smithay, que calcula geometry, exclusive zone e empilhamento. O compositor aplica
blur-behind; a web apenas declara a região.

## Frame em canvas fixo (ADR 0007)
Cada frame WebView tem canvas inicial >= output + insets; resize comum não redimensiona a WebView.
O compositor apresenta sempre 9-slice desse canvas no target `conteúdo + insets + shadow`. Crescimento
excepcional (`target > canvas`) é serializado por `frame_painted(serial)` após paint real; o ack do
configure não significa paint. Caps direito/inferior têm input mapeado do target para o canvas.
