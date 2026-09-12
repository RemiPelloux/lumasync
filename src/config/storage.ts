import { recordDiagnostic } from "../diagnostics";

const reported = new Set<string>();

export function readStored(key: string): unknown {
  try {
    const value = window.localStorage.getItem(key);
    return value === null ? null : JSON.parse(value);
  } catch (cause) {
    reportStorageError(key, cause);
    return null;
  }
}

export function writeStored(key: string, value: unknown) {
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
    reported.delete(key);
  } catch (cause) {
    reportStorageError(key, cause);
  }
}

function reportStorageError(key: string, cause: unknown) {
  if (reported.has(key)) return;
  reported.add(key);
  recordDiagnostic("warn", "storage.unavailable", `${key}: ${String(cause)}`);
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
