import { useState } from "react";
import { Activity, FileText, RefreshCw, Sparkles } from "lucide-react";
import { Button, StatusDot } from "./components";
import { useStudio } from "./hooks/useStudio";
import { BridgePanel } from "./views/BridgePanel";
import { LightsPanel } from "./views/LightsPanel";
import { SourcePanel } from "./views/SourcePanel";
import { SettingsPanel } from "./views/SettingsPanel";
import { SyncStage } from "./views/SyncStage";
import { DiagnosticsDialog } from "./views/DiagnosticsDialog";

export default function App() {
  const studio = useStudio();
  const { status, isRunning, loading, connected, error, statusError } = studio;
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const visibleError = error || (status.phase === "error" ? status.message : "");
  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand">
          <span className="brand__mark" aria-hidden="true"><Sparkles size={18} /></span>
          <span>LumaSync</span>
        </div>
        <div className="topbar__actions">
        <div className="topbar__status">
          <StatusDot tone={status.phase === "error" ? "error" : status.phase === "running" ? "success" : isRunning || loading ? "busy" : "idle"} />
          <span role="status" aria-live="polite">{status.phase === "error" ? "Synchronisation interrompue" : isRunning ? status.message : connected ? "Pont connecté" : "Configuration locale"}</span>
          {status.running && <span className="fps">{status.measuredFps.toFixed(0)} i/s</span>}
        </div>
        <Button variant="ghost" className="icon-button" icon={<FileText size={18} />} aria-label="Ouvrir le diagnostic" title="Diagnostic" onClick={() => setDiagnosticsOpen(true)} />
        </div>
      </header>

      {visibleError && <div className="error-banner app-alert" role="alert"><Activity size={18} /><span>{visibleError}</span><Button variant="ghost" className="icon-button" icon={<FileText size={16} />} aria-label="Consulter le journal d’erreurs" title="Consulter le journal" onClick={() => setDiagnosticsOpen(true)} /></div>}
      {statusError && <div className="error-banner app-alert app-alert--warning" role="status"><Activity size={18} /><span>{statusError}</span><Button variant="ghost" className="icon-button" icon={<RefreshCw size={16} />} aria-label="Réessayer la lecture du statut" title="Réessayer" onClick={() => void studio.refreshStatus()} /></div>}
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

        </aside>

        <SyncStage studio={studio} />
      </div>
      {diagnosticsOpen && <DiagnosticsDialog onClose={() => setDiagnosticsOpen(false)} />}
    </main>
  );
}
