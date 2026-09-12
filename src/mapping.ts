import type { ChannelAssignment, EntertainmentArea, MappingMode, Zone } from "./types";
import { recordDiagnostic } from "./diagnostics";

const MAPPING_KEY = "lumasync.mapping-mode.v1";

export const EDGE_ZONES: { id: Zone; label: string; short: string }[] = [
  { id: "left", label: "Gauche", short: "G" },
  { id: "right", label: "Droite", short: "D" },
  { id: "top", label: "Haut", short: "H" },
  { id: "bottom", label: "Bas", short: "B" },
];

export const CORNER_ZONES: { id: Zone; label: string; short: string }[] = [
  { id: "topLeft", label: "Haut gauche", short: "HG" },
  { id: "topRight", label: "Haut droit", short: "HD" },
  { id: "bottomLeft", label: "Bas gauche", short: "BG" },
  { id: "bottomRight", label: "Bas droit", short: "BD" },
];

export function zonesFor(mode: MappingMode) {
  switch (mode) {
    case "edges":
      return EDGE_ZONES;
    case "corners":
      return CORNER_ZONES;
    default: {
      const unreachable: never = mode;
      return unreachable;
    }
  }
}

export function loadMappingMode(): MappingMode {
  try {
    const stored = window.localStorage.getItem(MAPPING_KEY);
    return stored === "edges" || stored === "corners" ? stored : "corners";
  } catch (cause) {
    recordDiagnostic("warn", "storage.mapping.read", String(cause));
    return "corners";
  }
}

export function saveMappingMode(mode: MappingMode) {
  try {
    window.localStorage.setItem(MAPPING_KEY, mode);
  } catch (cause) {
    recordDiagnostic("warn", "storage.mapping.write", String(cause));
  }
}

export function buildAssignments(area: EntertainmentArea, mode: MappingMode): ChannelAssignment[] {
  const verticalAxis = detectVerticalAxis(area);
  return area.channels.map((channel) => ({
    channelId: channel.channelId,
    zone: preferredZone(channel.position, mode, verticalAxis),
  }));
}

type VerticalAxis = "y" | "z";

/** Hue uses y for screen height. Keep z as a compatibility fallback for old areas. */
function detectVerticalAxis(area: EntertainmentArea): VerticalAxis {
  if (area.channels.length < 2) return "y";
  const ys = area.channels.map((channel) => channel.position[1]);
  const zs = area.channels.map((channel) => channel.position[2]);
  const ySpread = Math.max(...ys) - Math.min(...ys);
  const zSpread = Math.max(...zs) - Math.min(...zs);
  return ySpread >= 0.2 || ySpread >= zSpread * 0.5 ? "y" : "z";
}

function preferredZone(position: [number, number, number], mode: MappingMode, verticalAxis: VerticalAxis): Zone {
  const [x, y, z] = position;
  const vertical = verticalAxis === "y" ? y : z;
  switch (mode) {
    case "edges":
      return Math.abs(x) >= Math.abs(vertical)
        ? x < 0
          ? "left"
          : "right"
        : vertical >= 0
          ? "top"
          : "bottom";
    case "corners": {
      return x < 0
        ? vertical >= 0
          ? "topLeft"
          : "bottomLeft"
        : vertical >= 0
          ? "topRight"
          : "bottomRight";
    }
    default: {
      const unreachable: never = mode;
      return unreachable;
    }
  }
}
