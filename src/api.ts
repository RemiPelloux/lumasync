import { invoke } from "@tauri-apps/api/core";
import type {
  BridgeInfo,
  EntertainmentArea,
  HueRoom,
  MonitorInfo,
  StartSyncRequest,
  SyncStatus,
} from "./types";

const tauriApi = {
  discoverBridges: () => invoke<BridgeInfo[]>("discover_bridges"),
  restoreBridge: (bridge: BridgeInfo) =>
    invoke<boolean>("restore_bridge", { bridge }),
  pairBridge: (bridge: BridgeInfo) =>
    invoke<void>("pair_bridge", { bridge }),
  getEntertainmentAreas: () =>
    invoke<EntertainmentArea[]>("get_entertainment_areas"),
  getHueRooms: () => invoke<HueRoom[]>("get_hue_rooms"),
  createEntertainmentFromRoom: (roomId: string) =>
    invoke<EntertainmentArea>("create_entertainment_from_room", { roomId }),
  getMonitors: () => invoke<MonitorInfo[]>("get_monitors"),
  startSync: (request: StartSyncRequest) =>
    invoke<void>("start_sync", { request }),
  stopSync: () => invoke<void>("stop_sync"),
  getSyncStatus: () => invoke<SyncStatus>("get_sync_status"),
};

let previewRunning = false;
let previewTick = 0;
const previewHasNoArea = new URLSearchParams(window.location.search).has("empty");
const previewBridge: BridgeInfo = {
  id: "PREVIEW001",
  host: "192.168.1.42",
  name: "Hue Bridge salon",
  port: 443,
};
const previewArea: EntertainmentArea = {
  id: "57b456d5-fb35-4f84-b658-32d8347419de",
  name: "Bureau",
  channels: [
    { channelId: 0, serviceId: "play-top-left", name: "Hue Play haut gauche", position: [-1, 0, 1] },
    { channelId: 1, serviceId: "play-top-right", name: "Hue Play haut droit", position: [1, 0, 1] },
    { channelId: 2, serviceId: "color-bottom-left", name: "Hue Color bas gauche", position: [-1, 0, -1] },
    { channelId: 3, serviceId: "color-bottom-right", name: "Hue Color bas droit", position: [1, 0, -1] },
  ],
};
const previewRoom: HueRoom = { id: "2", name: "Chambre", lightCount: 4 };

const previewApi = {
  discoverBridges: async () => [previewBridge],
  restoreBridge: async () => true,
  pairBridge: async () => undefined,
  getEntertainmentAreas: async () => previewHasNoArea ? [] : [previewArea],
  getHueRooms: async () => [previewRoom],
  createEntertainmentFromRoom: async () => ({ ...previewArea, name: "Chambre Ambilight" }),
  getMonitors: async () => [{ index: 0, name: "Écran principal", width: 1920, height: 1080, primary: true }],
  startSync: async () => { previewRunning = true; },
  stopSync: async () => { previewRunning = false; },
  getSyncStatus: async (): Promise<SyncStatus> => {
    previewTick += 0.16;
    const hue = (offset: number) => `hsl(${(previewTick * 38 + offset) % 360} 82% 61%)`;
    return {
      running: previewRunning,
      phase: previewRunning ? "running" : "idle",
      message: previewRunning ? "Éclairage synchronisé" : "Prêt à démarrer",
      measuredFps: previewRunning ? 29.8 : 0,
      frameTimeMs: previewRunning ? 7.4 : 0,
      droppedFrames: 0,
      blackBarsDetected: previewRunning && Math.sin(previewTick) > 0.25,
      colors: previewRunning
        ? { "0": hue(190), "1": hue(310), "2": hue(245), "3": hue(32) }
        : {},
    };
  },
};

const isTauri = "__TAURI_INTERNALS__" in window;

/** Browser preview data is kept outside packaged Tauri builds for visual QA. */
export const hueApi = isTauri ? tauriApi : previewApi;
