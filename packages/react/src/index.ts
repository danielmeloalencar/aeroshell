//! @aeroshell/react — adapters reativos do SDK para componentes React.

export {
  AeroProvider, mountSurface, useApps, useEvent, useOutputs, useSurface, useWindows, useWorkspaces,
} from "./adapters";
export { useIndicators, useNotifications, useSurfaces } from "./useWindows";
export type { Notification } from "../../sdk/src/generated/Notification";
export type { SurfaceSpec } from "../../sdk/src/generated/SurfaceSpec";
export type { WindowInfo } from "../../sdk/src/generated/WindowInfo";
export type { Snapshot } from "../../sdk/src/generated/Snapshot";
export type { SurfaceSession } from "../../sdk/src/surface-session";
export type { WindowStore } from "../../sdk/src/store";
