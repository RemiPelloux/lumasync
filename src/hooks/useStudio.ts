import { useCallback, useEffect, useMemo, useState } from "react";
import { hueApi } from "../api";
import { buildAssignments, loadMappingMode, saveMappingMode, zonesFor } from "../mapping";
import { loadSettings, profileFor, SETTINGS_KEY, INITIAL_STATUS, PROFILES, errorMessage } from "../config/profiles";
import type { RenderProfile } from "../config/profiles";
import type { BridgeInfo, ChannelAssignment, EntertainmentArea, HueRoom, MappingMode, MonitorInfo, StartSyncRequest, SyncSettings, Zone } from "../types";
import { useSyncStatus } from "./useSyncStatus";

export function useStudio() {
  const [bridges, setBridges] = useState<BridgeInfo[]>([]);
  const [bridge, setBridge] = useState<BridgeInfo | null>(null);
  const [manualHost, setManualHost] = useState("");
  const [connected, setConnected] = useState(false);
  const [pairingNeeded, setPairingNeeded] = useState(false);
  const [areas, setAreas] = useState<EntertainmentArea[]>([]);
  const [areaId, setAreaId] = useState("");
  const [rooms, setRooms] = useState<HueRoom[]>([]);
  const [roomId, setRoomId] = useState("");
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [monitorIndex, setMonitorIndex] = useState(0);
  const [assignments, setAssignments] = useState<ChannelAssignment[]>([]);
  const [mappingMode, setMappingMode] = useState(loadMappingMode);
  const [settings, setSettings] = useState(loadSettings);
  const zoneOptions = useMemo(() => zonesFor(mappingMode), [mappingMode]);
  const [status, setStatus] = useState(INITIAL_STATUS);
  const [loading, setLoading] = useState<"discover" | "connect" | "pair" | "data" | "create-area" | "start" | "stop" | null>(null);
  const [error, setError] = useState("");

  const activeArea = useMemo(
    () => areas.find((area) => area.id === areaId) ?? null,
    [areas, areaId],
  );
  const activeRoom = useMemo(
    () => rooms.find((room) => room.id === roomId) ?? null,
    [rooms, roomId],
  );
  const activeProfile = useMemo(() => profileFor(settings), [settings]);

  useEffect(() => {
    try {
      window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    } catch {
      // Les réglages restent utilisables pour la session si le stockage local est indisponible.
    }
  }, [settings]);

  useEffect(() => {
    saveMappingMode(mappingMode);
  }, [mappingMode]);

  const discover = useCallback(async () => {
    setLoading("discover");
    setError("");
    try {
      const found = await hueApi.discoverBridges();
      setBridges(found);
      if (found.length === 1) setBridge(found[0]);
      if (found.length === 0) {
        setError("Aucun Hue Bridge détecté. Vérifiez qu’il est relié à la même box que ce PC.");
      }
    } catch (cause) {
      setError(`La détection réseau a échoué : ${errorMessage(cause)}`);
    } finally {
      setLoading(null);
    }
  }, []);

  useEffect(() => {
    document.title = "Éclairage écran — LumaSync";
    void discover();
  }, [discover]);

  const loadBridgeData = useCallback(async () => {
    setLoading("data");
    setError("");
    try {
      const [foundAreas, foundMonitors] = await Promise.all([
        hueApi.getEntertainmentAreas(),
        hueApi.getMonitors(),
      ]);
      const foundRooms = foundAreas.length === 0 ? await hueApi.getHueRooms() : [];
      setAreas(foundAreas);
      setRooms(foundRooms);
      setMonitors(foundMonitors);
      const primary = foundMonitors.find((item) => item.primary) ?? foundMonitors[0];
      if (primary) setMonitorIndex(primary.index);
      if (foundAreas.length > 0) {
        setAreaId(foundAreas[0].id);
        setAssignments(buildAssignments(foundAreas[0], mappingMode));
      } else {
        setAreaId("");
        setAssignments([]);
        const bedroom = foundRooms.find((room) => room.name.localeCompare("Chambre", "fr", { sensitivity: "base" }) === 0);
        const suggestedRoom = bedroom ?? foundRooms[0];
        setRoomId(suggestedRoom?.id ?? "");
        if (!suggestedRoom) {
          setError("Aucune zone Entertainment ni pièce contenant des lampes n’a été trouvée sur ce pont.");
        }
      }
    } catch (cause) {
      setError(`Les lampes n’ont pas pu être chargées : ${errorMessage(cause)}`);
    } finally {
      setLoading(null);
    }
  }, [mappingMode]);

  const connect = async () => {
    if (!bridge) return;
    setLoading("connect");
    setError("");
    try {
      const restored = await hueApi.restoreBridge(bridge);
      if (!restored) {
        setPairingNeeded(true);
        return;
      }
      setConnected(true);
      setPairingNeeded(false);
      await loadBridgeData();
    } catch (cause) {
      setError(`Connexion impossible : ${errorMessage(cause)}`);
    } finally {
      setLoading(null);
    }
  };

  const useManualBridge = () => {
    const segments = manualHost.trim().split(".");
    const valid = segments.length === 4 && segments.every((segment) => {
      if (!/^\d{1,3}$/.test(segment)) return false;
      const value = Number(segment);
      return value >= 0 && value <= 255;
    });
    if (!valid) {
      setError("Entrez une adresse IPv4 valide, par exemple 192.168.1.42.");
      return;
    }
    const manualBridge: BridgeInfo = {
      id: "",
      host: manualHost.trim(),
      name: "Hue Bridge (adresse manuelle)",
      port: 443,
    };
    setBridges((current) => [manualBridge, ...current.filter((item) => item.host !== manualBridge.host)]);
    setBridge(manualBridge);
    setError("");
  };

  const pair = async () => {
    if (!bridge) return;
    setLoading("pair");
    setError("");
    try {
      await hueApi.pairBridge(bridge);
      setConnected(true);
      setPairingNeeded(false);
      await loadBridgeData();
    } catch (cause) {
      setError(`Association impossible : ${errorMessage(cause)}`);
    } finally {
      setLoading(null);
    }
  };

  useSyncStatus({ connected, status, setStatus });

  const chooseArea = (area: EntertainmentArea) => {
    setAreaId(area.id);
    setAssignments(buildAssignments(area, mappingMode));
  };

  const createAreaFromRoom = async () => {
    if (!roomId) return;
    setLoading("create-area");
    setError("");
    try {
      const area = await hueApi.createEntertainmentFromRoom(roomId);
      setAreas([area]);
      setAreaId(area.id);
      setAssignments(buildAssignments(area, mappingMode));
    } catch (cause) {
      setError(`La zone n’a pas pu être préparée : ${errorMessage(cause)}`);
    } finally {
      setLoading(null);
    }
  };

  const assignZone = (channelId: number, zone: Zone) => {
    setAssignments((current) =>
      current.map((assignment) =>
        assignment.channelId === channelId ? { ...assignment, zone } : assignment,
      ),
    );
  };

  const applyMappingMode = (mode: MappingMode) => {
    setMappingMode(mode);
    if (activeArea) setAssignments(buildAssignments(activeArea, mode));
  };

  const applyProfile = (profile: Exclude<RenderProfile, "custom">) => {
    const selected = PROFILES.find((item) => item.id === profile);
    if (selected) setSettings(selected.settings);
  };

  function updateSetting<Key extends keyof SyncSettings>(key: Key, value: SyncSettings[Key]) {
    setSettings((current) => ({ ...current, [key]: value }));
  }

  const start = async () => {
    if (!activeArea) return;
    setLoading("start");
    setError("");
    const request: StartSyncRequest = {
      areaId: activeArea.id,
      monitorIndex,
      assignments,
      ...settings,
    };
    try {
      setStatus((current) => ({ ...current, phase: "starting", message: "Ouverture du flux Hue…" }));
      await hueApi.startSync(request);
      setStatus(await hueApi.getSyncStatus());
    } catch (cause) {
      setStatus((current) => ({ ...current, running: false, phase: "error", message: "Démarrage interrompu" }));
      setError(`L’éclairage n’a pas démarré : ${errorMessage(cause)}`);
    } finally {
      setLoading(null);
    }
  };

  const stop = async () => {
    setLoading("stop");
    setError("");
    try {
      setStatus((current) => ({ ...current, phase: "stopping", message: "Arrêt du flux Hue…" }));
      await hueApi.stopSync();
      setStatus(await hueApi.getSyncStatus());
    } catch (cause) {
      setError(`L’arrêt du flux a rencontré un problème : ${errorMessage(cause)}`);
    } finally {
      setLoading(null);
    }
  };

  const colorByZone = useMemo(() => {
    const map = new Map<Zone, string>();
    for (const assignment of assignments) {
      map.set(assignment.zone, status.colors[String(assignment.channelId)] ?? "#343B63");
    }
    return map;
  }, [assignments, status.colors]);

  const colorForZone = (zone: Zone) => colorByZone.get(zone) ?? "#242A49";

  const isRunning = status.running || ["starting", "reconnecting", "stopping"].includes(status.phase);
  const ready = connected && activeArea && monitors.length > 0 && assignments.length > 0;

  return {
    bridges, bridge, setBridge, manualHost, setManualHost,
    connected, pairingNeeded, areas, areaId, rooms,
    roomId, setRoomId, monitors, monitorIndex, setMonitorIndex,
    assignments, mappingMode, settings, setSettings, zoneOptions,
    status, loading, error, activeArea, activeRoom,
    activeProfile, discover, connect, useManualBridge, pair,
    chooseArea, createAreaFromRoom, assignZone, applyMappingMode, applyProfile,
    updateSetting, start, stop, colorForZone, isRunning,
    ready,
  };
}
export type Studio = ReturnType<typeof useStudio>;
