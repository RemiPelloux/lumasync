import { useCallback, useEffect, useRef, useState } from "react";
import { errorMessage } from "../config/profiles";
import { recordDiagnostic } from "../diagnostics";

type Action = "discover" | "connect" | "pair" | "data" | "create-area" | "start" | "stop";

export function useStudioAction() {
  const [loading, setLoading] = useState<Action | null>(null);
  const [error, setError] = useState("");
  const busy = useRef(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  const run = useCallback(async (action: Action, message: string, operation: () => Promise<void>) => {
    if (busy.current) return;
    busy.current = true;
    setLoading(action);
    setError("");
    try {
      await operation();
    } catch (cause) {
      const detail = `${message} : ${errorMessage(cause)}`;
      recordDiagnostic("error", `studio.${action}`, detail);
      if (mounted.current) setError(detail);
    } finally {
      busy.current = false;
      if (mounted.current) setLoading(null);
    }
  }, []);

  return { loading, error, setError, busy, run };
}
