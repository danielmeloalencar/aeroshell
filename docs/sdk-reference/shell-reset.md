# `aero://shell-reset.css`

Folha de estilo **única**, servida pelo host da WebView pelo esquema `aero://` e injetada em
todas as superfícies visuais do AeroShell:

| Superfície | Status |
|---|---|
| `panel` | injetada (S2) |
| `desktop` | F2 |
| `overlay` | F2 |
| `frame` | F2/S5 |
| `lock` | F5 |
| `greeter` | F6 |

Conteúdo garantido:
- `box-sizing: border-box` global;
- `html, body { margin:0; padding:0; width:100%; height:100%; overflow:hidden;
  overscroll-behavior:none; }`;
- `user-select: none`, `cursor: default`, `-webkit-touch-callout: none`;
- `::-webkit-scrollbar { display:none; width:0; height:0; }` (elimina overlay scrollbar);
- fundo transparente por padrão.

**Componentes podem sobrescrever explicitamente** qualquer propriedade — por exemplo, um
widget de texto selecionável usa `user-select: text`, e um painel rolável usa
`overflow:auto` **e** estiliza a barra explicitamente. O reset é base, não restrição.

Bloqueios de interação do host (menu de contexto, zoom por gesto/Ctrl+scroll) são aplicados
pelo host WebKit além deste CSS; componentes não podem reabilitá-los por CSS sozinho.
