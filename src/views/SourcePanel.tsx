import { Check, Monitor } from "lucide-react";
import { Panel } from "../components";
import type { Studio } from "../hooks/useStudio";

export function SourcePanel({ studio }: { studio: Studio }) {
  const { monitors, monitorIndex, setMonitorIndex, isRunning } = studio;
  return (
    <Panel>
      <div className="panel__heading panel__heading--compact">
        <span className="icon-box"><Monitor size={19} /></span>
        <div><span className="step-label">Source</span><h2>Moniteur</h2></div>
      </div>
      <div className="choice-stack">
        {monitors.length === 0 && <p className="empty-state">{studio.isBusy && !studio.dataLoaded ? "Recherche des écrans…" : "Aucun écran disponible."}</p>}
        {monitors.map((item) => (
          <button
            key={item.index}
            type="button"
            className={`choice-row ${item.index === monitorIndex ? "choice-row--selected" : ""}`}
            onClick={() => setMonitorIndex(item.index)}
            aria-pressed={item.index === monitorIndex}
            disabled={isRunning || studio.isBusy}
          >
            <span><strong>{item.name}</strong><small>{item.width} × {item.height}{item.primary ? " · principal" : ""}</small></span>
            {item.index === monitorIndex && <Check size={17} aria-hidden="true" />}
          </button>
        ))}
      </div>
    </Panel>
  );
}
