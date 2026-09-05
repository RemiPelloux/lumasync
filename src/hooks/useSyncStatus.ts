import { startTransition, useEffect } from "react";
import type { Dispatch, SetStateAction } from "react";
import { hueApi } from "../api";
import type { SyncStatus } from "../types";

export function useSyncStatus({ connected, status, setStatus }: {
  connected: boolean;
  status: SyncStatus;
  setStatus: Dispatch<SetStateAction<SyncStatus>>;
}) {
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
}
