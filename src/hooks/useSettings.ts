import { useEffect, useRef, useState } from "react";
import { loadSettings, SETTINGS_KEY } from "../config/profiles";
import { writeStored } from "../config/storage";

const SETTINGS_SAVE_DELAY_MS = 300;

export function useSettings() {
  const [settings, setSettings] = useState(loadSettings);
  const pending = useRef({ settings, dirty: false });

  useEffect(() => {
    pending.current = { settings, dirty: true };
    const timer = window.setTimeout(flush, SETTINGS_SAVE_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [settings]);

  function flush() {
    if (!pending.current.dirty) return;
    writeStored(SETTINGS_KEY, pending.current.settings);
    pending.current.dirty = false;
  }

  useEffect(() => {
    const onVisibility = () => { if (document.hidden) flush(); };
    window.addEventListener("pagehide", flush);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      window.removeEventListener("pagehide", flush);
      document.removeEventListener("visibilitychange", onVisibility);
      flush();
    };
  }, []);

  return [settings, setSettings] as const;
}
