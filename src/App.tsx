import { startTransition, useCallback, useEffect, useMemo, useState } from "react";
import {
  Activity,
  ArrowRight,
  Check,
  CircleStop,
  Film,
  Gamepad2,
  Gauge,
  House,
  Leaf,
  Lightbulb,
  Link2,
  Monitor,
  Play,
  Plus,
  Radio,
  RefreshCw,
  RotateCcw,
  Router,
  SlidersHorizontal,
  Sparkles,
  Wifi,
} from "lucide-react";
import { hueApi } from "./api";
import { Button, Panel, StatusDot } from "./components";
import { buildAssignments, loadMappingMode, saveMappingMode, zonesFor } from "./mapping";
import type {
  BridgeInfo,
  ChannelAssignment,
  EntertainmentArea,
  HueRoom,
  MappingMode,
  MonitorInfo,
  StartSyncRequest,
  SyncSettings,
  SyncStatus,
  Zone,
} from "./types";

const DEFAULT_SETTINGS: SyncSettings = {
  brightness: 74,
  saturation: 100,
  reactivity: 82,
  maxLuminosity: 100,
  edgeDepth: 12,
  fps: 45,
  blackBarDetection: true,
};

type RenderProfile = "cinema" | "game" | "natural" | "custom";

const PROFILES: Array<{
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

const SETTINGS_KEY = "lumasync.render-settings.v4";

function bounded(value: unknown, fallback: number, min: number, max: number) {
  const numeric = Number(value);
  return Number.isFinite(numeric) ? Math.min(max, Math.max(min, numeric)) : fallback;
}

function loadSettings(): SyncSettings {
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

function profileFor(settings: SyncSettings): RenderProfile {
  return PROFILES.find((profile) => JSON.stringify(profile.settings) === JSON.stringify(settings))?.id ?? "custom";
}

const INITIAL_STATUS: SyncStatus = {
  running: false,
  phase: "idle",
  message: "Prêt à configurer",
  measuredFps: 0,
  frameTimeMs: 0,
  droppedFrames: 0,
  blackBarsDetected: false,
  colors: {},
};

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function App() {
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

  useEffect(() => {
    if (!connected) return;
    let cancelled = false;
    let timer = 0;
    let lastSignature = "";
    const poll = async () => {
      let active = false;
      try {
        const next = await hueApi.getSyncStatus();
        active = next.running || ["starting", "reconnecting", "stopping"].includes(next.phase);
        const signature = [
          next.phase,
          next.running ? "1" : "0",
          next.message,
          next.measuredFps.toFixed(1),
          next.frameTimeMs.toFixed(1),
          String(next.droppedFrames),
          next.blackBarsDetected ? "1" : "0",
          Object.entries(next.colors)
            .sort(([left], [right]) => left.localeCompare(right))
            .map(([id, color]) => `${id}:${color}`)
            .join("|"),
        ].join("·");
        if (!cancelled && signature !== lastSignature) {
          lastSignature = signature;
          startTransition(() => setStatus(next));
        }
      } catch {
        // Une perte ponctuelle de l'interface ne doit pas arrêter le flux Hue.
      } finally {
        if (!cancelled) {
          timer = window.setTimeout(() => void poll(), active ? 100 : 650);
        }
      }
    };
    void poll();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [connected, status.phase, status.running]);

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

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand">
          <span className="brand__mark" aria-hidden="true"><Sparkles size={18} /></span>
          <span>LumaSync</span>
        </div>
        <div className="topbar__status" role="status" aria-live="polite">
          <StatusDot tone={status.phase === "error" ? "error" : status.phase === "running" ? "success" : isRunning || loading ? "busy" : "idle"} />
          <span>{isRunning ? status.message : connected ? "Pont connecté" : "Configuration locale"}</span>
          {status.running && <span className="fps">{status.measuredFps.toFixed(0)} i/s</span>}
        </div>
      </header>

      <div className="workspace">
        <aside className="controls" aria-label="Configuration de l’éclairage">
          <div className="intro">
            <p className="eyebrow">Éclairage d’écran</p>
            <h1>Sync Hue · écran</h1>
          </div>

          {!connected ? (
            <Panel className="setup-panel">
              <div className="panel__heading">
                <span className="icon-box"><Router size={19} /></span>
                <div><span className="step-label">Étape 1</span><h2>Hue Bridge</h2></div>
              </div>

              <div className="bridge-list" aria-live="polite">
                {bridges.map((item) => (
                  <button
                    key={`${item.id}-${item.host}`}
                    type="button"
                    className={`bridge-card ${bridge?.host === item.host ? "bridge-card--selected" : ""}`}
                    onClick={() => setBridge(item)}
                    aria-pressed={bridge?.host === item.host}
                  >
                    <Wifi size={18} />
                    <span><strong>{item.name || "Hue Bridge"}</strong><small>{item.host}</small></span>
                    {bridge?.host === item.host && <Check size={18} aria-hidden="true" />}
                  </button>
                ))}
              </div>

              <div className="manual-bridge">
                <label htmlFor="bridge-address">Adresse IP manuelle</label>
                <div className="manual-bridge__control">
                  <input
                    id="bridge-address"
                    type="text"
                    inputMode="numeric"
                    autoComplete="off"
                    spellCheck={false}
                    placeholder="192.168.1.42"
                    value={manualHost}
                    onChange={(event) => setManualHost(event.currentTarget.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && !event.nativeEvent.isComposing) useManualBridge();
                    }}
                  />
                  <Button variant="secondary" disabled={!manualHost.trim()} onClick={useManualBridge}>
                    Utiliser
                  </Button>
                </div>
              </div>

              {pairingNeeded && (
                <div className="pairing-callout" role="status">
                  <span className="bridge-button-illustration" aria-hidden="true"><span /></span>
                  <div><strong>Appuyez sur le bouton central</strong><p>Puis lancez l’association dans les 30 secondes.</p></div>
                </div>
              )}

              <div className="action-row">
                <Button variant="ghost" icon={<RefreshCw size={18} />} busy={loading === "discover"} onClick={() => void discover()}>
                  Détecter
                </Button>
                {pairingNeeded ? (
                  <Button variant="primary" icon={<Link2 size={18} />} busy={loading === "pair"} onClick={() => void pair()}>
                    Associer le pont
                  </Button>
                ) : (
                  <Button variant="primary" icon={<ArrowRight size={18} />} busy={loading === "connect"} disabled={!bridge} onClick={() => void connect()}>
                    Continuer
                  </Button>
                )}
              </div>
            </Panel>
          ) : (
            <>
              <Panel>
                <div className="panel__heading panel__heading--compact">
                  <span className="icon-box"><Lightbulb size={19} /></span>
                  <div><span className="step-label">Zone Hue</span><h2>Lumières</h2></div>
                </div>

                <div className="choice-stack">
                  {areas.map((area) => (
                    <button
                      key={area.id}
                      type="button"
                      className={`choice-row ${area.id === areaId ? "choice-row--selected" : ""}`}
                      onClick={() => chooseArea(area)}
                      aria-pressed={area.id === areaId}
                      disabled={isRunning}
                    >
                      <span><strong>{area.name}</strong><small>{area.channels.length} canaux</small></span>
                      {area.id === areaId && <Radio size={17} aria-hidden="true" />}
                    </button>
                  ))}
                </div>

                {areas.length === 0 && rooms.length > 0 && (
                  <div className="room-recovery">
                    <div className="room-recovery__message">
                      <House size={18} aria-hidden="true" />
                      <div>
                        <strong>Pièce Hue détectée</strong>
                        <p>« {activeRoom?.name ?? "Cette pièce"} » est une pièce. Le streaming nécessite une zone Entertainment séparée.</p>
                      </div>
                    </div>
                    <div className="choice-stack" aria-label="Pièce à utiliser">
                      {rooms.map((room) => (
                        <button
                          key={room.id}
                          type="button"
                          className={`choice-row ${room.id === roomId ? "choice-row--selected" : ""}`}
                          onClick={() => setRoomId(room.id)}
                          aria-pressed={room.id === roomId}
                          disabled={loading === "create-area"}
                        >
                          <span><strong>{room.name}</strong><small>{room.lightCount} lampe{room.lightCount > 1 ? "s" : ""}</small></span>
                          {room.id === roomId && <Check size={17} aria-hidden="true" />}
                        </button>
                      ))}
                    </div>
                    <p className="room-recovery__note">La pièce reste intacte. LumaSync ajoute seulement la configuration requise au pont.</p>
                    <Button
                      variant="primary"
                      icon={<Plus size={18} />}
                      busy={loading === "create-area"}
                      disabled={!roomId}
                      onClick={() => void createAreaFromRoom()}
                    >
                      Créer depuis {activeRoom?.name ?? "cette pièce"}
                    </Button>
                  </div>
                )}

                {activeArea && (
                  <div className="mapping-list">
                    <div className="mapping-mode">
                      <div className="cadence-control">
                        <span>Disposition</span>
                        <div className="option-picker" role="group" aria-label="Disposition des lumières">
                          {([
                            { id: "corners", label: "Angles" },
                            { id: "edges", label: "Bords" },
                          ] as const).map((mode) => (
                            <button
                              key={mode.id}
                              type="button"
                              aria-pressed={mappingMode === mode.id}
                              className={mappingMode === mode.id ? "option-button option-button--active" : "option-button"}
                              disabled={isRunning}
                              onClick={() => applyMappingMode(mode.id)}
                            >
                              {mode.label}
                            </button>
                          ))}
                        </div>
                      </div>
                      <p className="mapping-mode__hint">
                        {mappingMode === "corners"
                          ? "Chaque lumière suit un coin de l’écran (HG, HD, BG, BD)."
                          : "Chaque lumière suit un côté de l’écran (G, D, H, B)."}
                      </p>
                    </div>
                    {activeArea.channels.map((channel) => {
                      const current = assignments.find((item) => item.channelId === channel.channelId)?.zone;
                      return (
                        <div className="mapping-row" key={channel.channelId}>
                          <span className="light-name">{channel.name}</span>
                          <div
                            className={`zone-picker ${mappingMode === "corners" ? "zone-picker--corners" : ""}`}
                            aria-label={`Position de ${channel.name}`}
                          >
                            {zoneOptions.map((zone) => (
                              <button
                                key={zone.id}
                                type="button"
                                className={current === zone.id ? "zone-button zone-button--active" : "zone-button"}
                                onClick={() => assignZone(channel.channelId, zone.id)}
                                aria-label={zone.label}
                                aria-pressed={current === zone.id}
                                disabled={isRunning}
                              >{zone.short}</button>
                            ))}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </Panel>

              <Panel>
                <div className="panel__heading panel__heading--compact">
                  <span className="icon-box"><Monitor size={19} /></span>
                  <div><span className="step-label">Source</span><h2>Moniteur</h2></div>
                </div>
                <div className="choice-stack">
                  {monitors.map((item) => (
                    <button
                      key={item.index}
                      type="button"
                      className={`choice-row ${item.index === monitorIndex ? "choice-row--selected" : ""}`}
                      onClick={() => setMonitorIndex(item.index)}
                      aria-pressed={item.index === monitorIndex}
                      disabled={isRunning}
                    >
                      <span><strong>{item.name}</strong><small>{item.width} × {item.height}{item.primary ? " · principal" : ""}</small></span>
                      {item.index === monitorIndex && <Check size={17} aria-hidden="true" />}
                    </button>
                  ))}
                </div>
              </Panel>

              <Panel>
                <div className="panel__heading panel__heading--compact panel__heading--action">
                  <span className="icon-box"><SlidersHorizontal size={19} /></span>
                  <div><span className="step-label">Rendu</span><h2>Réglages</h2></div>
                  <Button
                    variant="ghost"
                    className="button--compact"
                    icon={<RotateCcw size={15} />}
                    disabled={isRunning}
                    onClick={() => setSettings(DEFAULT_SETTINGS)}
                  >
                    Réinitialiser
                  </Button>
                </div>
                <div className="profile-picker" role="group" aria-label="Profil de rendu">
                  {PROFILES.map((profile) => {
                    const ProfileIcon = profile.icon;
                    return (
                      <button
                        key={profile.id}
                        type="button"
                        className={`profile-button ${activeProfile === profile.id ? "profile-button--active" : ""}`}
                        aria-pressed={activeProfile === profile.id}
                        disabled={isRunning}
                        onClick={() => applyProfile(profile.id)}
                      >
                        <ProfileIcon size={16} aria-hidden="true" />
                        <span><strong>{profile.label}</strong><small>{profile.detail}</small></span>
                      </button>
                    );
                  })}
                </div>
                <div className="sliders">
                  <RangeControl label="Luminosité" value={settings.brightness} min={10} max={100} suffix="%" disabled={isRunning} onChange={(value) => updateSetting("brightness", value)} />
                  <RangeControl label="Luminosité max" value={settings.maxLuminosity} min={20} max={100} suffix="%" disabled={isRunning} onChange={(value) => updateSetting("maxLuminosity", value)} />
                  <RangeControl label="Saturation" value={settings.saturation} min={40} max={150} suffix="%" disabled={isRunning} onChange={(value) => updateSetting("saturation", value)} />
                  <RangeControl label="Réactivité" value={settings.reactivity} min={10} max={100} suffix="%" disabled={isRunning} onChange={(value) => updateSetting("reactivity", value)} />
                  <RangeControl label="Profondeur d’analyse" value={settings.edgeDepth} min={5} max={30} suffix="%" disabled={isRunning} onChange={(value) => updateSetting("edgeDepth", value)} />
                </div>
                <div className="render-options">
                  <div className="cadence-control">
                    <span>Cadence cible</span>
                    <div className="option-picker" role="group" aria-label="Cadence cible">
                      {[30, 45, 60].map((fps) => (
                        <button
                          key={fps}
                          type="button"
                          aria-pressed={settings.fps === fps}
                          className={settings.fps === fps ? "option-button option-button--active" : "option-button"}
                          disabled={isRunning}
                          onClick={() => updateSetting("fps", fps)}
                        >
                          {fps}
                        </button>
                      ))}
                    </div>
                  </div>
                  <button
                    type="button"
                    className="toggle-row"
                    aria-pressed={settings.blackBarDetection}
                    disabled={isRunning}
                    onClick={() => updateSetting("blackBarDetection", !settings.blackBarDetection)}
                  >
                    <span><strong>Bandes noires automatiques</strong><small>Utile en film ; désactivez en jeu 4K pour gagner des i/s</small></span>
                    <span className={`toggle-indicator ${settings.blackBarDetection ? "toggle-indicator--on" : ""}`} aria-hidden="true"><span /></span>
                  </button>
                </div>
              </Panel>
            </>
          )}

          {error && <div className="error-banner" role="alert"><Activity size={18} /><span>{error}</span></div>}
        </aside>

        <section className="stage" aria-label="Aperçu de la synchronisation">
          <div className="stage__meta">
            <div><span className="eyebrow">Aperçu en direct</span><h2>{activeArea?.name ?? "Votre installation"}</h2></div>
            <span className={`mode-chip ${status.running ? "mode-chip--active" : ""}`}>
              <StatusDot tone={status.phase === "error" ? "error" : status.phase === "reconnecting" ? "busy" : status.running ? "success" : "idle"} />
              {status.phase === "reconnecting" ? "Reconnexion" : status.running ? "En direct" : "En attente"}
            </span>
          </div>

          <div className="telemetry" aria-label="Mesures de synchronisation">
            <div><Gauge size={15} aria-hidden="true" /><span><small>Cadence</small><strong>{status.running ? `${status.measuredFps.toFixed(1)} i/s` : `${settings.fps} cible`}</strong></span></div>
            <div><span className="telemetry__tick" aria-hidden="true" /><span><small>Traitement</small><strong>{status.running ? `${status.frameTimeMs.toFixed(1)} ms` : "En attente"}</strong></span></div>
            <div><span className="telemetry__tick" aria-hidden="true" /><span><small>Cadre utile</small><strong>{status.blackBarsDetected ? "Bandes retirées" : settings.blackBarDetection ? "Auto" : "Plein écran"}</strong></span></div>
            <div><span className="telemetry__tick" aria-hidden="true" /><span><small>Stabilité</small><strong>{status.running ? status.droppedFrames > 0 ? `${status.droppedFrames} retard${status.droppedFrames > 1 ? "s" : ""}` : "Stable" : "En attente"}</strong></span></div>
          </div>

          <div className={`screen-scene ${status.running ? "screen-scene--live" : ""}`}>
            {zoneOptions.map((zone) => (
              <span
                key={`ambient-${zone.id}`}
                className={`ambient ambient--${zone.id}`}
                style={{ "--ambient": colorForZone(zone.id) } as React.CSSProperties}
              />
            ))}
            <div className="monitor-frame">
              {zoneOptions.map((zone) => (
                <span
                  key={`marker-${zone.id}`}
                  className={`edge-marker edge-marker--${zone.id}`}
                  style={{ "--marker": colorForZone(zone.id) } as React.CSSProperties}
                  aria-hidden="true"
                >
                  {zone.short}
                </span>
              ))}
              <div className="monitor-screen">
                <div className="screen-orbit" aria-hidden="true" />
                <span className="screen-kicker">Capture locale</span>
                <strong>{status.running ? "La lumière suit votre écran" : connected ? "Prêt pour l’image" : "Connectez votre pont"}</strong>
                <small>{status.running ? `${status.measuredFps.toFixed(0)} images analysées par seconde` : "Aucune image ne quitte ce PC"}</small>
              </div>
              <span className="monitor-led" aria-hidden="true" />
            </div>
          </div>

          <div className="stage__footer">
            <div className="privacy-note"><Wifi size={17} /><span>Flux direct PC → Hue Bridge, sur votre réseau local.</span></div>
            {isRunning ? (
              <Button variant="danger" icon={<CircleStop size={19} />} busy={loading === "stop" || status.phase === "stopping"} onClick={() => void stop()}>
                Arrêter l’éclairage
              </Button>
            ) : (
              <Button variant="primary" icon={<Play size={19} />} busy={loading === "start" || status.phase === "starting"} disabled={!ready || loading === "data"} onClick={() => void start()}>
                Démarrer l’éclairage
              </Button>
            )}
          </div>
        </section>
      </div>
    </main>
  );
}

interface RangeControlProps {
  label: string;
  value: number;
  min: number;
  max: number;
  suffix: string;
  disabled?: boolean;
  onChange: (value: number) => void;
}

function RangeControl({ label, value, min, max, suffix, disabled, onChange }: RangeControlProps) {
  const id = `range-${label.toLowerCase().replaceAll(" ", "-")}`;
  const fill = ((value - min) / (max - min)) * 100;
  return (
    <div className="range-control">
      <div className="range-control__label"><label htmlFor={id}>{label}</label><output htmlFor={id}>{value}{suffix}</output></div>
      <input
        id={id}
        type="range"
        min={min}
        max={max}
        value={value}
        disabled={disabled}
        style={{ "--range-fill": `${fill}%` } as React.CSSProperties}
        onChange={(event) => onChange(Number(event.currentTarget.value))}
      />
    </div>
  );
}

export default App;
