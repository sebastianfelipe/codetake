import { useCallback, useEffect, useRef, useState } from "react";
import { defaultPreset, deserializePreset, type Preset } from "../features/settings/preset";
import { api } from "../lib/api";

const SAVE_DELAY_MS = 400;

/** The saved preset, loaded once and saved (debounced) whenever it changes. */
export function useSettings() {
  const [preset, setPreset] = useState<Preset>(defaultPreset);
  const [loaded, setLoaded] = useState(false);
  const saveTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    api
      .loadSettings()
      .then((saved) => setPreset(deserializePreset(saved)))
      .catch(() => setPreset(defaultPreset))
      .finally(() => setLoaded(true));
  }, []);

  useEffect(() => {
    if (!loaded) {
      return;
    }
    window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => {
      api.saveSettings(preset).catch((error) => console.warn("could not save settings", error));
    }, SAVE_DELAY_MS);
    return () => window.clearTimeout(saveTimer.current);
  }, [preset, loaded]);

  const update = useCallback((change: (current: Preset) => Preset) => setPreset(change), []);

  return { preset, update, loaded };
}
