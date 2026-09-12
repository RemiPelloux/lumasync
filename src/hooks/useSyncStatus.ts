import { startTransition, useCallback, useEffect, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { hueApi } from "../api";
import { errorMessage } from "../config/profiles";
import { recordDiagnostic } from "../diagnostics";
import type { SyncStatus } from "../types";

const ACTIVE_POLL_MS = 150;
const IDLE_POLL_MS = 1_500;
const HIDDEN_POLL_MS = 5_000;
const MAX_RETRY_MS = 8_000;

export function statusesEqual(previous: SyncStatus, next: SyncStatus) {
  if (previous.phase !== next.phase || previous.running !== next.running || previous.message !== next.message
    || Math.round(previous.measuredFps * 10) !== Math.round(next.measuredFps * 10)
    || Math.round(previous.frameTimeMs * 10) !== Math.round(next.frameTimeMs * 10)
    || previous.droppedFrames !== next.droppedFrames || previous.blackBarsDetected !== next.blackBarsDetected) return false;
  const keys = Object.keys(next.colors);
  return keys.length === Object.keys(previous.colors).length
    && keys.every((key) => previous.colors[key] === next.colors[key]);
}

export function useSyncStatus({ connected, paused, setStatus }: {
  connected: boolean;
  paused: boolean;
  setStatus: Dispatch<SetStateAction<SyncStatus>>;
}) {
  const [statusError, setStatusError] = useState("");
  const pending = useRef<Promise<SyncStatus> | null>(null);
  const refresh = useRef<(() => Promise<void>) | null>(null);
  const refreshStatus = useCallback(async () => { await refresh.current?.(); }, []);

  useEffect(() => {
    if (!connected) setStatusError("");
    if (!connected || paused) return;
    let cancelled = false;
    let polling = false;
    let timer = 0;
    let failures = 0;
    let active = false;
    let reportedPhase = "";

    const poll = async () => {
      if (polling || cancelled) return;
      window.clearTimeout(timer);
      polling = true;
      // Drain a request from the previous connection/action before fetching fresh state.
      if (pending.current) await pending.current.catch(() => undefined);
      if (cancelled) return;
      const request = Promise.resolve().then(() => hueApi.getSyncStatus());
      pending.current = request;
      try {
        const next = await request;
        if (cancelled) return;
        active = next.running || ["starting", "reconnecting", "stopping"].includes(next.phase);
        if (failures) recordDiagnostic("info", "status.recovered", "Le suivi du flux est de nouveau disponible.");
        failures = 0;
        setStatusError("");
        if (next.phase !== reportedPhase && ["error", "reconnecting"].includes(next.phase)) {
          recordDiagnostic(next.phase === "error" ? "error" : "warn", `sync.${next.phase}`, next.message);
        }
        reportedPhase = next.phase;
        startTransition(() => setStatus((current) => cancelled || statusesEqual(current, next) ? current : next));
      } catch (cause) {
        if (cancelled) return;
        failures += 1;
        const message = `Le suivi du flux est indisponible : ${errorMessage(cause)}`;
        setStatusError(message);
        if (failures === 1) recordDiagnostic("warn", "status.poll.failed", message);
      } finally {
        if (pending.current === request) pending.current = null;
        polling = false;
        if (!cancelled) {
          const delay = failures ? Math.min(MAX_RETRY_MS, 500 * 2 ** Math.min(failures, 4))
            : active ? ACTIVE_POLL_MS : IDLE_POLL_MS;
          timer = window.setTimeout(() => void poll(), document.hidden ? Math.max(HIDDEN_POLL_MS, delay) : delay);
        }
      }
    };
    const onVisibility = () => { void poll(); };
    refresh.current = poll;
    document.addEventListener("visibilitychange", onVisibility);
    void poll();
    return () => {
      cancelled = true;
      refresh.current = null;
      window.clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [connected, paused, setStatus]);

  return { statusError, refreshStatus };
}
