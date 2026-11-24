//! Regras declarativas da política de decoração (F2-decoration-policy-ts).
//! Única fonte de decisão: o compositor não conhece nenhum app_id específico.
//! Carregadas no startup; hot-reload fica para cartão próprio.

import type { DecorationPolicyConfig } from "../../sdk/src/generated/DecorationPolicyConfig";

const config: DecorationPolicyConfig = {
  default: "Server",
  rules: [
    // GTK4/gedit desenha a própria moldura: sem barra dupla, sem flash.
    { app_id: null, class: "gedit", policy: "Client" },
    // foot aceita a moldura do compositor.
    { app_id: "foot", class: null, policy: "Server" },
    // Cliente sintético de teste: nenhuma moldura.
    { app_id: "aero-no-deco", class: null, policy: "None" },
  ],
};

export default config;
