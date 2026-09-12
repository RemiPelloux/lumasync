import { useCallback, useEffect, useMemo, useState } from "react";
import type { SetStateAction } from "react";
import { hueApi } from "../api";
import { loadMappingMode, saveMappingMode, zonesFor } from "../mapping";
import { profileFor, INITIAL_STATUS, PROFILES, normalizeSettings } from "../config/profiles";
import type { RenderProfile } from "../config/profiles";
import { bridgeKey, preferredBridge, rememberAssignments, rememberBridge, rememberSelection, restoreAssignments, restoreSelection } from "../config/session";
import type { BridgeInfo, ChannelAssignment, EntertainmentArea, HueRoom, MappingMode, MonitorInfo, StartSyncRequest, SyncSettings, Zone } from "../types";
import { useSyncStatus } from "./useSyncStatus";
import { useSettings } from "./useSettings";
import { useStudioAction } from "./useStudioAction";

export function useStudio() {
  const [bridges, setBridges] = useState<BridgeInfo[]>([]);
  const [bridge, setSelectedBridge] = useState<BridgeInfo | null>(null);
  const [manualHost, setManualHost] = useState("");
  const [connected, setConnected] = useState(false);
  const [pairingNeeded, setPairingNeeded] = useState(false);
  const [dataLoaded, setDataLoaded] = useState(false);
  const [areas, setAreas] = useState<EntertainmentArea[]>([]);
  const [areaId, setAreaId] = useState("");
  const [rooms, setRooms] = useState<HueRoom[]>([]);
  const [roomId, changeRoomId] = useState("");
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [monitorIndex, changeMonitorIndex] = useState(0);
  const [assignments, setAssignments] = useState<ChannelAssignment[]>([]);
  const [mappingMode, setMappingMode] = useState(loadMappingMode);
  const [settings, changeSettings] = useSettings();
  const [status, setStatus] = useState(INITIAL_STATUS);
  const { loading, error, setError, busy, run } = useStudioAction();
  const { statusError, refreshStatus } = useSyncStatus({ connected, paused: loading !== null, setStatus });
  const zoneOptions = useMemo(() => zonesFor(mappingMode), [mappingMode]);
  const activeArea = useMemo(() => areas.find((area) => area.id === areaId) ?? null, [areas, areaId]);
  const activeRoom = useMemo(() => rooms.find((room) => room.id === roomId) ?? null, [rooms, roomId]);
  const activeProfile = useMemo(() => profileFor(settings), [settings]);
  const isRunning = status.running || ["starting", "reconnecting", "stopping"].includes(status.phase);
  const ready = connected && dataLoaded && !!activeArea && monitors.some((item) => item.index === monitorIndex)
    && assignments.length > 0 && assignments.length === activeArea.channels.length;

  const discover = useCallback(() => run("discover", "La détection réseau a échoué", async () => {
    const found = await hueApi.discoverBridges();
    setBridges(found);
    setSelectedBridge((current) => current ?? preferredBridge(found));
    if (!found.length) setError("Aucun Hue Bridge détecté. Vérifiez qu’il est relié à la même box que ce PC.");
  }), [run, setError]);

  useEffect(() => {
    document.title = "Éclairage écran — LumaSync";
    void discover();
  }, [discover]);

  const setBridge = (selected: BridgeInfo) => {
    if (busy.current || isRunning || (bridge && bridgeKey(bridge) === bridgeKey(selected) && bridge.host === selected.host)) return;
    setSelectedBridge(selected);
    rememberBridge(selected);
    setConnected(false);
    setPairingNeeded(false);
    setDataLoaded(false);
    setAreas([]);
    setRooms([]);
    setMonitors([]);
    setAreaId("");
    setAssignments([]);
    setStatus(INITIAL_STATUS);
    setError("");
  };

  const fetchBridgeData = async (selected: BridgeInfo) => {
    const [foundAreas, foundMonitors] = await Promise.all([hueApi.getEntertainmentAreas(), hueApi.getMonitors()]);
    const foundRooms = foundAreas.length ? [] : await hueApi.getHueRooms();
    const restored = restoreSelection(selected, { areas: foundAreas, monitors: foundMonitors });
    const monitor = foundMonitors.find((item) => item.index === restored.monitorIndex);
    rememberSelection(selected, { ...(restored.area ? { areaId: restored.area.id } : {}), ...(monitor ? { monitor } : {}) });
    setAreas(foundAreas);
    setRooms(foundRooms);
    setMonitors(foundMonitors);
    changeMonitorIndex(restored.monitorIndex);
    setAreaId(restored.area?.id ?? "");
    setAssignments(restored.area ? restoreAssignments(selected, restored.area, mappingMode) : []);
    const bedroom = foundRooms.find((room) => room.name.localeCompare("Chambre", "fr", { sensitivity: "base" }) === 0);
    changeRoomId((bedroom ?? foundRooms[0])?.id ?? "");
    setDataLoaded(true);
    if (!foundAreas.length && !foundRooms.length) setError("Aucune zone Entertainment ni pièce contenant des lampes n’a été trouvée sur ce pont.");
    else if (!foundMonitors.length) setError("Aucun écran disponible. Rebranchez un moniteur puis actualisez les sources.");
  };

  const loadBridgeData = async () => {
    if (!bridge || !connected || isRunning) return;
    await run("data", "Les sources n’ont pas pu être chargées", async () => {
      setDataLoaded(false);
      await fetchBridgeData(bridge);
    });
  };

  const connect = async () => {
    if (!bridge || isRunning) return;
    await run("connect", "Connexion impossible", async () => {
      const restored = await hueApi.restoreBridge(bridge);
      setPairingNeeded(!restored);
      setConnected(restored);
      if (!restored) return;
      rememberBridge(bridge);
      await fetchBridgeData(bridge);
    });
  };

  const useManualBridge = () => {
    if (busy.current || isRunning) return;
    const segments = manualHost.trim().split(".");
    const valid = segments.length === 4 && segments.every((segment) => /^\d{1,3}$/.test(segment) && Number(segment) <= 255);
    if (!valid) {
      setError("Entrez une adresse IPv4 valide, par exemple 192.168.1.42.");
      return;
    }
    const host = segments.map(Number).join(".");
    const manualBridge = bridges.find((item) => item.host === host)
      ?? { id: "", host, name: "Hue Bridge (adresse manuelle)", port: 443 };
    setBridges((current) => [manualBridge, ...current.filter((item) => item.host !== host)]);
    setBridge(manualBridge);
    setError("");
  };

  const pair = async () => {
    if (!bridge || !pairingNeeded || isRunning) return;
    await run("pair", "Association impossible", async () => {
      await hueApi.pairBridge(bridge);
      setConnected(true);
      setPairingNeeded(false);
      rememberBridge(bridge);
      await fetchBridgeData(bridge);
    });
  };

  const chooseArea = (area: EntertainmentArea) => {
    if (!bridge || busy.current || isRunning || !areas.some((item) => item.id === area.id)) return;
    setAreaId(area.id);
    setAssignments(restoreAssignments(bridge, area, mappingMode));
    rememberSelection(bridge, { areaId: area.id });
  };

  const createAreaFromRoom = async () => {
    if (!bridge || !connected || !activeRoom || isRunning) return;
    await run("create-area", "La zone n’a pas pu être préparée", async () => {
      const area = await hueApi.createEntertainmentFromRoom(roomId);
      setAreas((current) => [...current.filter((item) => item.id !== area.id), area]);
      setAreaId(area.id);
      setAssignments(restoreAssignments(bridge, area, mappingMode));
      rememberSelection(bridge, { areaId: area.id });
    });
  };

  const setRoomId = (id: string) => {
    if (!busy.current && !isRunning && rooms.some((room) => room.id === id)) changeRoomId(id);
  };

  const setMonitorIndex = (index: number) => {
    if (!bridge || busy.current || isRunning) return;
    const monitor = monitors.find((item) => item.index === index);
    if (!monitor) return;
    changeMonitorIndex(index);
    rememberSelection(bridge, { monitor });
  };

  const assignZone = (channelId: number, zone: Zone) => {
    if (!bridge || !activeArea || busy.current || isRunning || !zoneOptions.some((item) => item.id === zone)) return;
    const next = assignments.map((assignment) => assignment.channelId === channelId ? { ...assignment, zone } : assignment);
    setAssignments(next);
    rememberAssignments(bridge, activeArea, { mode: mappingMode, assignments: next });
  };

  const applyMappingMode = (mode: MappingMode) => {
    if (busy.current || isRunning || (mode !== "edges" && mode !== "corners")) return;
    setMappingMode(mode);
    saveMappingMode(mode);
    if (bridge && activeArea) setAssignments(restoreAssignments(bridge, activeArea, mode));
  };

  const setSettings = (next: SetStateAction<SyncSettings>) => {
    if (busy.current || isRunning) return;
    changeSettings((current) => normalizeSettings(typeof next === "function" ? next(current) : next));
  };

  const applyProfile = (profile: Exclude<RenderProfile, "custom">) => {
    const selected = PROFILES.find((item) => item.id === profile);
    if (selected) setSettings(selected.settings);
  };

  function updateSetting<Key extends keyof SyncSettings>(key: Key, value: SyncSettings[Key]) {
    setSettings((current) => ({ ...current, [key]: value }));
  }

  const start = async () => {
    if (!ready || !activeArea || isRunning) return;
    const request: StartSyncRequest = { areaId: activeArea.id, monitorIndex, assignments, ...settings };
    await run("start", "L’éclairage n’a pas démarré", async () => {
      setStatus((current) => ({ ...current, phase: "starting", message: "Ouverture du flux Hue…" }));
      try {
        await hueApi.startSync(request);
        setStatus((current) => ({ ...current, running: true, phase: "running", message: "Éclairage synchronisé" }));
      } catch (cause) {
        setStatus((current) => ({ ...current, running: false, phase: "error", message: "Démarrage interrompu" }));
        throw cause;
      }
    });
  };

  const stop = async () => {
    if (!connected) return;
    await run("stop", "L’arrêt du flux a rencontré un problème", async () => {
      setStatus((current) => ({ ...current, phase: "stopping", message: "Arrêt du flux Hue…" }));
      try {
        await hueApi.stopSync();
        setStatus({ ...INITIAL_STATUS, message: "Prêt à démarrer" });
      } catch (cause) {
        setStatus((current) => ({ ...current, phase: "error", message: "Arrêt non confirmé" }));
        throw cause;
      }
    });
  };

  const colorByZone = useMemo(() => {
    const map = new Map<Zone, string>();
    for (const assignment of assignments) map.set(assignment.zone, status.colors[String(assignment.channelId)] ?? "#343B63");
    return map;
  }, [assignments, status.colors]);
  const colorForZone = (zone: Zone) => colorByZone.get(zone) ?? "#242A49";

  return {
    bridges, bridge, setBridge, manualHost, setManualHost, connected, pairingNeeded, areas, areaId, rooms,
    roomId, setRoomId, monitors, monitorIndex, setMonitorIndex, assignments, mappingMode, settings, setSettings, zoneOptions,
    status, statusError, refreshStatus, loading, error, dataLoaded, loadBridgeData, activeArea, activeRoom,
    activeProfile, discover, connect, useManualBridge, pair, chooseArea, createAreaFromRoom, assignZone, applyMappingMode,
    applyProfile, updateSetting, start, stop, colorForZone, isRunning, isBusy: loading !== null, ready,
  };
}
export type Studio = ReturnType<typeof useStudio>;
