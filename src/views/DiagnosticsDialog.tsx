import { useCallback, useEffect, useRef, useState } from "react";
import { Download, RefreshCw, X } from "lucide-react";
import { Button } from "../components";
import { readDiagnostics } from "../diagnostics";
import type { Diagnostics } from "../diagnostics";

const LEVEL_LABELS = { info: "Info", warn: "Attention", error: "Erreur" };
const timeFormat = new Intl.DateTimeFormat("fr-FR", { hour: "2-digit", minute: "2-digit", second: "2-digit" });

function EntryTime({ timestamp }: { timestamp: number }) {
  const date = new Date(timestamp);
  if (!Number.isFinite(date.getTime())) return <span>Heure inconnue</span>;
  return <time dateTime={date.toISOString()}>{timeFormat.format(date)}</time>;
}

export function DiagnosticsDialog({ onClose }: { onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [data, setData] = useState<Diagnostics>({ entries: [], logPath: null });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [problemsOnly, setProblemsOnly] = useState(false);
  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try { setData(await readDiagnostics()); }
    catch { setError("Le journal est indisponible. Réessayez dans quelques instants."); }
    finally { setLoading(false); }
  }, []);

  useEffect(() => {
    dialog.current?.showModal();
    void refresh();
  }, [refresh]);

  const download = () => {
    const blob = new Blob([data.entries.map((entry) => JSON.stringify(entry)).join("\n") + "\n"], { type: "application/x-ndjson" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `lumasync-diagnostic-${new Date().toISOString().replaceAll(":", "-")}.jsonl`;
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  };

  const entries = data.entries.filter((entry) => !problemsOnly || entry.level !== "info").slice().reverse();
  return (
    <dialog ref={dialog} className="diagnostics-dialog" aria-labelledby="diagnostics-title" onCancel={onClose} onClose={onClose}>
      <header className="diagnostics-heading">
        <h2 id="diagnostics-title">Diagnostic</h2>
        <Button variant="ghost" className="icon-button" icon={<X size={18} />} aria-label="Fermer le diagnostic" title="Fermer" onClick={() => dialog.current?.close()} />
      </header>
      <div className="diagnostics-toolbar">
        <label><input type="checkbox" checked={problemsOnly} onChange={(event) => setProblemsOnly(event.currentTarget.checked)} /> Problèmes uniquement</label>
        <div>
          <Button variant="ghost" className="icon-button" icon={<RefreshCw size={17} />} busy={loading} aria-label="Actualiser le journal" title="Actualiser" onClick={() => void refresh()} />
          <Button variant="ghost" className="icon-button" icon={<Download size={17} />} disabled={!data.entries.length} aria-label="Exporter le journal" title="Exporter le journal" onClick={download} />
        </div>
      </div>
      {error && <p className="diagnostics-error" role="alert">{error}</p>}
      <div className="diagnostics-entries" aria-busy={loading} tabIndex={0}>
        {!entries.length && <p className="empty-state">{loading ? "Lecture du journal…" : "Aucun événement à afficher."}</p>}
        {entries.map((entry, index) => (
          <article className={`diagnostic-entry diagnostic-entry--${entry.level}`} key={`${entry.timestampMs}-${index}`}>
            <div><EntryTime timestamp={entry.timestampMs} /><strong>{LEVEL_LABELS[entry.level]}</strong><span>{entry.component}</span></div>
            <p>{entry.message}</p>
          </article>
        ))}
      </div>
      <footer className="diagnostics-footer">{data.logPath ?? "Journal de cette session d’aperçu"}</footer>
    </dialog>
  );
}
