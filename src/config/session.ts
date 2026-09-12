import { buildAssignments, zonesFor } from "../mapping";
import type { BridgeInfo, ChannelAssignment, EntertainmentArea, MappingMode, MonitorInfo } from "../types";
import { isRecord, readStored, writeStored } from "./storage";

const SESSION_KEY = "lumasync.workspace.v1";

export function bridgeKey(bridge: BridgeInfo): string {
  return bridge.id ? `id:${bridge.id.toLowerCase()}` : `host:${bridge.host}:${bridge.port}`;
}

function readSession() {
  const stored = readStored(SESSION_KEY);
  return isRecord(stored) ? stored : {};
}

function bridgeSession(bridge: BridgeInfo) {
  const stored = readSession()[bridgeKey(bridge)];
  return isRecord(stored) ? stored : {};
}

function saveBridgeSession(bridge: BridgeInfo, value: Record<string, unknown>) {
  const session = readSession();
  session[bridgeKey(bridge)] = value;
  session.lastBridge = bridgeKey(bridge);
  writeStored(SESSION_KEY, session);
}

export function preferredBridge(bridges: BridgeInfo[]) {
  const key = readSession().lastBridge;
  return bridges.find((bridge) => bridgeKey(bridge) === key) ?? (bridges.length === 1 ? bridges[0] : null);
}

export function rememberBridge(bridge: BridgeInfo) {
  saveBridgeSession(bridge, bridgeSession(bridge));
}

export function restoreSelection(bridge: BridgeInfo, data: { areas: EntertainmentArea[]; monitors: MonitorInfo[] }) {
  const saved = bridgeSession(bridge);
  const area = data.areas.find((item) => item.id === saved.areaId) ?? data.areas[0] ?? null;
  const monitor = saved.monitor;
  const matching = isRecord(monitor) ? data.monitors.filter((item) =>
    item.name === monitor.name && item.width === monitor.width && item.height === monitor.height) : [];
  const selected = matching.find((item) => isRecord(monitor) && item.index === monitor.index)
    ?? matching[0] ?? data.monitors.find((item) => item.primary) ?? data.monitors[0];
  return { area, monitorIndex: selected?.index ?? 0 };
}

export function rememberSelection(bridge: BridgeInfo, selection: { areaId?: string; monitor?: MonitorInfo }) {
  saveBridgeSession(bridge, { ...bridgeSession(bridge), ...selection });
}

export function restoreAssignments(bridge: BridgeInfo, area: EntertainmentArea, mode: MappingMode): ChannelAssignment[] {
  const mappings = bridgeSession(bridge).mappings;
  const savedArea = isRecord(mappings) ? mappings[area.id] : null;
  const saved = isRecord(savedArea) ? savedArea[mode] : null;
  const allowed = new Set(zonesFor(mode).map((zone) => zone.id));
  const byChannel = new Map<number, ChannelAssignment>();
  if (Array.isArray(saved)) {
    for (const value of saved) {
      if (!isRecord(value)) continue;
      const channel = area.channels.find((item) => item.channelId === value.channelId && item.serviceId === value.serviceId);
      if (channel && allowed.has(value.zone as ChannelAssignment["zone"])) {
        byChannel.set(channel.channelId, { channelId: channel.channelId, zone: value.zone as ChannelAssignment["zone"] });
      }
    }
  }
  return buildAssignments(area, mode).map((assignment) => byChannel.get(assignment.channelId) ?? assignment);
}

export function rememberAssignments(bridge: BridgeInfo, area: EntertainmentArea, data: { mode: MappingMode; assignments: ChannelAssignment[] }) {
  const saved = bridgeSession(bridge);
  const mappings = isRecord(saved.mappings) ? saved.mappings : {};
  const previousArea = mappings[area.id];
  const savedArea = isRecord(previousArea) ? previousArea : {};
  mappings[area.id] = { ...savedArea, [data.mode]: data.assignments.map((assignment) => ({
    ...assignment, serviceId: area.channels.find((item) => item.channelId === assignment.channelId)?.serviceId,
  })) };
  saveBridgeSession(bridge, { ...saved, mappings, areaId: area.id });
}
