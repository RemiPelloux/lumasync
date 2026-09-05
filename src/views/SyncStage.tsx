import { CircleStop, Gauge, Play, Wifi } from "lucide-react";
import { Button, StatusDot } from "../components";
import type { Studio } from "../hooks/useStudio";

export function SyncStage({ studio }: { studio: Studio }) {
  const { connected, settings, zoneOptions, status, loading, activeArea, start, stop, colorForZone, isRunning, ready } = studio;
  return (
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
            <small>{status.running ? `${status.measuredFps.toFixed(0)} envois par seconde` : "Aucune image ne quitte ce PC"}</small>
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
  );
}
