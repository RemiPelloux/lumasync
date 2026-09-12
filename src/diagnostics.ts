import { invoke } from "@tauri-apps/api/core";

export interface DiagnosticEntry {
  timestampMs: number;
  level: "info" | "warn" | "error";
  component: string;
  message: string;
}

export interface Diagnostics {
  entries: DiagnosticEntry[];
  logPath: string | null;
}

const native = "__TAURI_INTERNALS__" in window;
const browserEntries: DiagnosticEntry[] = [];
const recent = new Map<string, number>();
const MAX_ENTRIES = 200;
const REPEAT_INTERVAL_MS = 30_000;

export function reportProblem(component: string, message: string) {
  recordDiagnostic("error", component, message);
}

export function recordDiagnostic(level: DiagnosticEntry["level"], component: string, message: string) {
  const key = `${component}:${message}`;
  const now = Date.now();
  if (now - (recent.get(key) ?? 0) < REPEAT_INTERVAL_MS) return;
  if (recent.size >= MAX_ENTRIES) recent.clear();
  recent.set(key, now);
  if (native) {
    void invoke("log_frontend_event", { component, message, level }).catch(() => undefined);
  } else {
    browserEntries.push({ timestampMs: now, level, component, message });
    if (browserEntries.length > MAX_ENTRIES) browserEntries.shift();
  }
}

export function readDiagnostics(): Promise<Diagnostics> {
  return native ? invoke("get_diagnostics") : Promise.resolve({ entries: [...browserEntries], logPath: null });
}

export function installErrorLogging() {
  // Only categorical context crosses IPC; exception text may contain credentials.
  window.addEventListener("error", () => reportProblem("interface", "Erreur JavaScript non interceptée."));
  window.addEventListener("unhandledrejection", () => reportProblem("interface", "Promesse rejetée sans gestionnaire."));
}
