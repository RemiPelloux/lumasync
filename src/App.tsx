import { Activity, Sparkles } from "lucide-react";
import { StatusDot } from "./components";
import { useStudio } from "./hooks/useStudio";
import { BridgePanel } from "./views/BridgePanel";
import { LightsPanel } from "./views/LightsPanel";
import { SourcePanel } from "./views/SourcePanel";
import { SettingsPanel } from "./views/SettingsPanel";
import { SyncStage } from "./views/SyncStage";

export default function App() {
  const studio = useStudio();
  const { status, isRunning, loading, connected, error } = studio;
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
            <BridgePanel studio={studio} />
          ) : (
            <>
              <LightsPanel studio={studio} />

              <SourcePanel studio={studio} />

              <SettingsPanel studio={studio} />
            </>
          )}

          {error && <div className="error-banner" role="alert"><Activity size={18} /><span>{error}</span></div>}
        </aside>

        <SyncStage studio={studio} />
      </div>
    </main>
  );
}
