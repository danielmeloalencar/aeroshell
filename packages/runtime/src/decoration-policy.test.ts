//! F2-decoration-policy-ts: teste-first das regras declarativas tipadas.
import { expect, test } from "bun:test";

import { DECORATION_WIRE, decideDecoration, matchesClass } from "./decoration-policy";
import rules from "./rules";

function request(appId: string) {
  return {
    id: 0,
    app_id: appId,
    title: `policy-${appId}`,
    mode: "normal",
    client_frame: false,
  };
}

test("gedit → Client pela classe; foot → Server pelo app_id; aero-no-deco → None", () => {
  expect(decideDecoration(rules, request("gedit"))).toEqual({ policy: "Client", source: "rule" });
  expect(decideDecoration(rules, request("foot"))).toEqual({ policy: "Server", source: "rule" });
  expect(decideDecoration(rules, request("aero-no-deco"))).toEqual({
    policy: "None",
    source: "rule",
  });
});

test("app_id desconhecido usa o default declarado em TS", () => {
  expect(decideDecoration(rules, request("aero-unknown"))).toEqual({
    policy: "Server",
    source: "default",
  });
});

test("classe casa com app_id cheio ou com o último segmento (Wayland não tem WM_CLASS)", () => {
  expect(matchesClass("gedit", "gedit")).toBe(true);
  expect(matchesClass("gedit", "org.gnome.gedit")).toBe(true);
  expect(matchesClass("Gedit", "org.gnome.GEDIT")).toBe(true);
  expect(matchesClass("foot", "gedit")).toBe(false);
});

test("primeira regra que casa vence; regra sem seletor casa qualquer janela", () => {
  const config = {
    default: "Server" as const,
    rules: [
      { app_id: null, class: null, policy: "Client" as const },
      { app_id: "foot", class: null, policy: "None" as const },
    ],
  };
  expect(decideDecoration(config, request("foot"))).toEqual({ policy: "Client", source: "rule" });
});

test("conversão tipada para a wire minúscula do IPC", () => {
  expect(DECORATION_WIRE.Server).toBe("server");
  expect(DECORATION_WIRE.Client).toBe("client");
  expect(DECORATION_WIRE.None).toBe("none");
});
