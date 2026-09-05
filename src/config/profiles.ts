import { Film, Gamepad2, Leaf } from "lucide-react";
import type { SyncSettings, SyncStatus } from "../types";

export const DEFAULT_SETTINGS: SyncSettings = {
  brightness: 74,
  saturation: 100,
  reactivity: 82,
  maxLuminosity: 100,
  edgeDepth: 12,
  fps: 45,
  blackBarDetection: true,
};

export type RenderProfile = "cinema" | "game" | "natural" | "custom";

export const PROFILES: Array<{
  id: Exclude<RenderProfile, "custom">;
  label: string;
  detail: string;
  icon: typeof Film;
  settings: SyncSettings;
}> = [
  {
    id: "cinema",
    label: "Cinéma",
    detail: "Fluide",
    icon: Film,
    settings: { brightness: 70, saturation: 106, reactivity: 56, maxLuminosity: 78, edgeDepth: 10, fps: 30, blackBarDetection: true },
  },
  {
    id: "game",
    label: "Jeu",
    detail: "Rapide",
    icon: Gamepad2,
    settings: { brightness: 82, saturation: 114, reactivity: 92, maxLuminosity: 100, edgeDepth: 8, fps: 60, blackBarDetection: false },
  },
  {
    id: "natural",
    label: "Naturel",
    detail: "Fidèle",
    icon: Leaf,
    settings: DEFAULT_SETTINGS,
  },
];

export const SETTINGS_KEY = "lumasync.render-settings.v4";

function bounded(value: unknown, fallback: number, min: number, max: number) {
  const numeric = Number(value);
  return Number.isFinite(numeric) ? Math.min(max, Math.max(min, numeric)) : fallback;
}

export function loadSettings(): SyncSettings {
  try {
    const parsed = JSON.parse(window.localStorage.getItem(SETTINGS_KEY) ?? "null") as Partial<SyncSettings> | null;
    if (!parsed) {
      // Migrate v3 settings if present.
      const legacy = JSON.parse(window.localStorage.getItem("lumasync.render-settings.v3") ?? "null") as Partial<SyncSettings> | null;
      if (!legacy) return DEFAULT_SETTINGS;
      return {
        brightness: bounded(legacy.brightness, DEFAULT_SETTINGS.brightness, 10, 100),
        saturation: bounded(legacy.saturation, DEFAULT_SETTINGS.saturation, 40, 150),
        reactivity: bounded(legacy.reactivity, DEFAULT_SETTINGS.reactivity, 10, 100),
        maxLuminosity: DEFAULT_SETTINGS.maxLuminosity,
        edgeDepth: bounded(legacy.edgeDepth, DEFAULT_SETTINGS.edgeDepth, 5, 30),
        fps: [30, 45, 60].includes(Number(legacy.fps)) ? Number(legacy.fps) : DEFAULT_SETTINGS.fps,
        blackBarDetection: typeof legacy.blackBarDetection === "boolean" ? legacy.blackBarDetection : true,
      };
    }
    return {
      brightness: bounded(parsed.brightness, DEFAULT_SETTINGS.brightness, 10, 100),
      saturation: bounded(parsed.saturation, DEFAULT_SETTINGS.saturation, 40, 150),
      reactivity: bounded(parsed.reactivity, DEFAULT_SETTINGS.reactivity, 10, 100),
      maxLuminosity: bounded(parsed.maxLuminosity, DEFAULT_SETTINGS.maxLuminosity, 20, 100),
      edgeDepth: bounded(parsed.edgeDepth, DEFAULT_SETTINGS.edgeDepth, 5, 30),
      fps: [30, 45, 60].includes(Number(parsed.fps)) ? Number(parsed.fps) : DEFAULT_SETTINGS.fps,
      blackBarDetection: typeof parsed.blackBarDetection === "boolean" ? parsed.blackBarDetection : true,
    };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export function profileFor(settings: SyncSettings): RenderProfile {
  return PROFILES.find((profile) => JSON.stringify(profile.settings) === JSON.stringify(settings))?.id ?? "custom";
}

export const INITIAL_STATUS: SyncStatus = {
  running: false,
  phase: "idle",
  message: "Prêt à configurer",
  measuredFps: 0,
  frameTimeMs: 0,
  droppedFrames: 0,
  blackBarsDetected: false,
  colors: {},
};

export function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
