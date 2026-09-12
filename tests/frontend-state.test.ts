import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import type { BridgeInfo, EntertainmentArea, MonitorInfo } from "../src/types";

const storage = new Map<string, string>();
Object.defineProperty(globalThis, "window", { configurable: true, value: {
  location: { search: "" },
  localStorage: {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
  },
} });

const { buildAssignments } = await import("../src/mapping");
const { DEFAULT_SETTINGS, INITIAL_STATUS, SETTINGS_KEY, loadSettings, normalizeSettings, profileFor } = await import("../src/config/profiles");
const { bridgeKey, preferredBridge, rememberAssignments, rememberBridge, rememberSelection, restoreAssignments, restoreSelection } = await import("../src/config/session");
const { statusesEqual } = await import("../src/hooks/useSyncStatus");

const bridge: BridgeInfo = { id: "BRIDGE01", host: "192.168.1.42", name: "Salon", port: 443 };
const area: EntertainmentArea = { id: "area-1", name: "Bureau", channels: [
  { channelId: 0, serviceId: "light-1", name: "Gauche 1", position: [-1, 0, 0.5] },
  { channelId: 1, serviceId: "light-2", name: "Gauche 2", position: [-1, 0, 0.8] },
] };
const monitors: MonitorInfo[] = [
  { index: 0, name: "Portable", width: 1920, height: 1080, primary: true },
  { index: 1, name: "Externe", width: 2560, height: 1440, primary: false },
];

beforeEach(() => storage.clear());

test("automatic mapping keeps lamps on their physical side even when they share a zone", () => {
  assert.deepEqual(buildAssignments(area, "edges"), [
    { channelId: 0, zone: "left" }, { channelId: 1, zone: "left" },
  ]);
  assert.deepEqual(buildAssignments(area, "corners").map((item) => item.zone), ["topLeft", "topLeft"]);
  assert.deepEqual(buildAssignments({ ...area, channels: [...area.channels].reverse() }, "edges").reverse(), buildAssignments(area, "edges"));
});

test("automatic mapping uses Hue y axis so bottom lamps follow the lower screen", () => {
  const areaWithVerticalHueCoordinates: EntertainmentArea = {
    ...area,
    channels: [
      { channelId: 0, serviceId: "top", name: "Haut", position: [0, 1, 0] },
      { channelId: 1, serviceId: "bottom", name: "Bas", position: [0, -1, 0] },
    ],
  };
  assert.deepEqual(buildAssignments(areaWithVerticalHueCoordinates, "edges"), [
    { channelId: 0, zone: "top" }, { channelId: 1, zone: "bottom" },
  ]);
  assert.deepEqual(buildAssignments(areaWithVerticalHueCoordinates, "corners").map((item) => item.zone), ["topRight", "bottomRight"]);
});

test("legacy areas with z as vertical remain compatible", () => {
  const legacy = {
    ...area,
    channels: [
      { channelId: 0, serviceId: "top", name: "Haut", position: [0, 1, 1] },
      { channelId: 1, serviceId: "bottom", name: "Bas", position: [0, 1, -1] },
    ],
  };
  assert.deepEqual(buildAssignments(legacy, "edges").map((item) => item.zone), ["top", "bottom"]);
});

test("settings reject coerced values and clamp valid numbers", () => {
  const settings = normalizeSettings({ brightness: null, saturation: "75", reactivity: false, edgeDepth: Infinity, maxLuminosity: 300, fps: 144 });
  assert.deepEqual(settings, { ...DEFAULT_SETTINGS, maxLuminosity: 100 });
  assert.equal(normalizeSettings({ brightness: -3 }).brightness, 10);
  assert.equal(normalizeSettings({ blackBarDetection: false }).blackBarDetection, false);
});

test("corrupt current settings recover from valid legacy settings", () => {
  storage.set(SETTINGS_KEY, "broken json");
  storage.set("lumasync.render-settings.v3", JSON.stringify({ brightness: 65, fps: 30 }));
  assert.equal(loadSettings().brightness, 65);
  assert.equal(loadSettings().fps, 30);
  assert.equal(loadSettings().maxLuminosity, DEFAULT_SETTINGS.maxLuminosity);
});

test("profile detection is independent of object property insertion order", () => {
  const reordered = Object.fromEntries(Object.entries(DEFAULT_SETTINGS).reverse());
  assert.equal(profileFor(reordered as typeof DEFAULT_SETTINGS), "natural");
  assert.equal(profileFor({ ...DEFAULT_SETTINGS, brightness: 65 }), "custom");
});

test("assignments restore only within the same bridge, area and mapping mode", () => {
  const assignments = [{ channelId: 0, zone: "bottomRight" as const }, { channelId: 1, zone: "topRight" as const }];
  rememberAssignments(bridge, area, { mode: "corners", assignments });
  assert.deepEqual(restoreAssignments(bridge, area, "corners"), assignments);
  assert.deepEqual(restoreAssignments({ ...bridge, id: "OTHER" }, area, "corners"), buildAssignments(area, "corners"));
  assert.deepEqual(restoreAssignments(bridge, { ...area, id: "area-2" }, "corners"), buildAssignments(area, "corners"));
  assert.deepEqual(restoreAssignments(bridge, area, "edges"), buildAssignments(area, "edges"));
});

test("replaced lamps do not inherit another lamp's channel assignment", () => {
  rememberAssignments(bridge, area, { mode: "corners", assignments: [{ channelId: 0, zone: "bottomRight" }] });
  const replaced = { ...area, channels: [{ ...area.channels[0], serviceId: "replacement" }, area.channels[1]] };
  assert.deepEqual(restoreAssignments(bridge, replaced, "corners"), buildAssignments(replaced, "corners"));
});

test("invalid saved zones and unavailable channels are discarded", () => {
  storage.set("lumasync.workspace.v1", JSON.stringify({ [bridgeKey(bridge)]: { mappings: { [area.id]: { corners: [
    { channelId: 0, serviceId: "light-1", zone: "left" },
    { channelId: 30, serviceId: "light-1", zone: "topRight" },
    null,
  ] } } } }));
  assert.deepEqual(restoreAssignments(bridge, area, "corners"), buildAssignments(area, "corners"));
});

test("monitor restoration follows monitor identity when indices change", () => {
  rememberSelection(bridge, { areaId: area.id, monitor: monitors[1] });
  const reordered = [{ ...monitors[0], index: 1 }, { ...monitors[1], index: 0 }];
  assert.equal(restoreSelection(bridge, { areas: [area], monitors: reordered }).monitorIndex, 0);
  assert.equal(restoreSelection(bridge, { areas: [area], monitors }).monitorIndex, 1);
});

test("unavailable sources fall back to the first area and primary monitor", () => {
  rememberSelection(bridge, { areaId: "removed", monitor: { ...monitors[1], name: "Removed" } });
  assert.deepEqual(restoreSelection(bridge, { areas: [area], monitors }), { area, monitorIndex: 0 });
  assert.deepEqual(restoreSelection(bridge, { areas: [], monitors: [] }), { area: null, monitorIndex: 0 });
});

test("malformed session data does not prevent source or assignment restoration", () => {
  for (const malformed of ["invalid", "[]", "null", JSON.stringify({ [bridgeKey(bridge)]: { mappings: [1] } })]) {
    storage.set("lumasync.workspace.v1", malformed);
    assert.deepEqual(restoreAssignments(bridge, area, "corners"), buildAssignments(area, "corners"));
    assert.equal(restoreSelection(bridge, { areas: [area], monitors }).monitorIndex, 0);
  }
});

test("blocked storage keeps settings and mapping usable for the session", () => {
  const read = window.localStorage.getItem;
  const write = window.localStorage.setItem;
  window.localStorage.getItem = () => { throw new Error("Storage blocked"); };
  window.localStorage.setItem = () => { throw new Error("Storage blocked"); };
  try {
    assert.deepEqual(loadSettings(), DEFAULT_SETTINGS);
    assert.deepEqual(restoreAssignments(bridge, area, "edges"), buildAssignments(area, "edges"));
    assert.doesNotThrow(() => rememberSelection(bridge, { areaId: area.id }));
  } finally {
    window.localStorage.getItem = read;
    window.localStorage.setItem = write;
  }
});

test("bridge preference follows stable identity without automatically choosing among unknown bridges", () => {
  const other = { ...bridge, id: "OTHER", host: "192.168.1.43" };
  assert.equal(preferredBridge([bridge, other]), null);
  rememberBridge(bridge);
  const moved = { ...bridge, host: "192.168.1.44" };
  assert.equal(preferredBridge([other, moved]), moved);
});

test("status comparison skips invisible noise but preserves errors, color removal and frame loss", () => {
  const status = { ...INITIAL_STATUS, measuredFps: 45.01, colors: { "0": "#ffffff", "1": "#000000" } };
  assert.equal(statusesEqual(status, { ...status, measuredFps: 45.02, colors: { "1": "#000000", "0": "#ffffff" } }), true);
  assert.equal(statusesEqual(status, { ...status, phase: "error" }), false);
  assert.equal(statusesEqual(status, { ...status, colors: { "0": "#ffffff" } }), false);
  assert.equal(statusesEqual(status, { ...status, droppedFrames: 1 }), false);
});
