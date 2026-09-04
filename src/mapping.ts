import type { ChannelAssignment, EntertainmentArea, MappingMode, Zone } from "./types";

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
  } catch {
    return "corners";
  }
}

export function saveMappingMode(mode: MappingMode) {
  try {
    window.localStorage.setItem(MAPPING_KEY, mode);
  } catch {
    // Le mode reste utilisable pour la session si le stockage local est indisponible.
  }
}

export function buildAssignments(area: EntertainmentArea, mode: MappingMode): ChannelAssignment[] {
  const unused = new Set<Zone>(zonesFor(mode).map((zone) => zone.id));
  const assignments: ChannelAssignment[] = [];

  for (const channel of area.channels) {
    const preferred = preferredZone(channel.position, mode);
    const zone = unused.has(preferred) ? preferred : (unused.values().next().value ?? preferred);
    unused.delete(zone);
    assignments.push({ channelId: channel.channelId, zone });
  }

  return assignments;
}

function preferredZone(position: [number, number, number], mode: MappingMode): Zone {
  const [x, , z] = position;
  switch (mode) {
    case "edges":
      return Math.abs(x) >= Math.abs(z)
        ? x < 0
          ? "left"
          : "right"
        : z >= 0
          ? "top"
          : "bottom";
    case "corners": {
      // Prefer the dominant axis so edge midpoints still map to four distinct corners.
      if (Math.abs(x) > Math.abs(z) + 0.05) {
        return x < 0
          ? z >= 0
            ? "topLeft"
            : "bottomLeft"
          : z >= 0
            ? "topRight"
            : "bottomRight";
      }
      if (Math.abs(z) > Math.abs(x) + 0.05) {
        return z >= 0
          ? x < 0
            ? "topLeft"
            : "topRight"
          : x < 0
            ? "bottomLeft"
            : "bottomRight";
      }
      return x < 0
        ? z >= 0
          ? "topLeft"
          : "bottomLeft"
        : z >= 0
          ? "topRight"
          : "bottomRight";
    }
    default: {
      const unreachable: never = mode;
      return unreachable;
    }
  }
}
