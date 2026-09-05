import { ArrowRight, Check, Link2, RefreshCw, Router, Wifi } from "lucide-react";
import { Button, Panel } from "../components";
import type { Studio } from "../hooks/useStudio";

export function BridgePanel({ studio }: { studio: Studio }) {
  const { bridges, bridge, setBridge, manualHost, setManualHost, pairingNeeded, loading, discover, connect, useManualBridge, pair } = studio;
  return (
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
  );
}
