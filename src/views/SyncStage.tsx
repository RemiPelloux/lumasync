import { CircleStop, Gauge, Play, Monitor, PowerOff } from "lucide-react";
import { Button, StatusDot } from "../components";
import type { Studio } from "../hooks/useStudio";

export function SyncStage({ studio }: { studio: Studio }) {
  const { connected, settings, zoneOptions, status, loading, activeArea, start, stop, colorForZone, isRunning, ready } = studio;
  const monitor = studio.monitors.find((item) => item.index === studio.monitorIndex);
  const phaseLabel = { idle: "En attente", starting: "Démarrage", running: "En direct", reconnecting: "Reconnexion", stopping: "Arrêt en cours", error: "Interrompu" }[status.phase];
  const idleLabel = status.phase === "error" ? "Synchronisation interrompue" : isRunning ? phaseLabel : !connected ? "Pont à connecter" : !activeArea ? "Zone Hue à sélectionner" : !monitor ? "Écran indisponible" : "Prêt à démarrer";
  return (
    <section className="stage" aria-label="Aperçu de la synchronisation">
      <div className="stage__meta">
        <div><span className="eyebrow">Aperçu en direct</span><h2>{activeArea?.name ?? "Votre installation"}</h2></div>
        <span className={`mode-chip ${status.running ? "mode-chip--active" : ""}`}>
          <StatusDot tone={status.phase === "error" ? "error" : status.phase === "reconnecting" ? "busy" : status.running ? "success" : "idle"} />
          {phaseLabel}
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
          <div className={`monitor-screen ${status.running ? "monitor-screen--live" : ""}`}>
            {status.running ? (
              <div className={`color-preview color-preview--${studio.mappingMode}`} aria-label="Couleurs envoyées aux lumières">
                {zoneOptions.map((zone) => <div key={zone.id} className={`color-preview__zone color-preview__zone--${zone.id}`} style={{ backgroundColor: colorForZone(zone.id) }}><span>{zone.label}</span></div>)}
              </div>
            ) : <><Monitor size={28} className="monitor-idle-icon" aria-hidden="true" /><strong>{idleLabel}</strong><small>{monitor ? `${monitor.width} × ${monitor.height}` : ""}</small></>}
          </div>
          <span className="monitor-led" aria-hidden="true" />
        </div>
      </div>

      <div className="stage__footer">
        <div className="privacy-note"><Monitor size={17} /><span>{monitor?.name ?? "Aucun écran sélectionné"}</span></div>
        <div className="stage__commands">
        {status.phase === "error" && connected && <Button variant="secondary" icon={<PowerOff size={17} />} busy={loading === "stop"} disabled={studio.isBusy} onClick={() => void stop()}>Réessayer l’arrêt</Button>}
        {isRunning ? (
          <Button variant="danger" icon={<CircleStop size={19} />} busy={loading === "stop" || status.phase === "stopping"} disabled={loading === "start" || status.phase === "starting"} onClick={() => void stop()}>
            Arrêter l’éclairage
          </Button>
        ) : (
          <Button variant="primary" icon={<Play size={19} />} busy={loading === "start" || status.phase === "starting"} disabled={!ready || studio.isBusy} onClick={() => void start()}>
            Démarrer l’éclairage
          </Button>
        )}
        </div>
      </div>
    </section>
  );
}
