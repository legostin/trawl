import { create } from "zustand";

const seenKey = (pluginId: string) => `trawl.spawnConsent.${pluginId}`;

interface SpawnConsentState {
  open: boolean;
  pluginId: string;
  command: string;
  _resolve: ((granted: boolean) => void) | null;
  /** Ask once per plugin before it starts its first child process. Resolves
   *  true instantly on later calls once the user has agreed. */
  request: (pluginId: string, command: string) => Promise<boolean>;
  confirm: () => void;
  cancel: () => void;
}

export const useSpawnConsent = create<SpawnConsentState>((set, get) => ({
  open: false,
  pluginId: "",
  command: "",
  _resolve: null,
  request: (pluginId, command) => {
    try {
      if (localStorage.getItem(seenKey(pluginId))) return Promise.resolve(true);
    } catch {
      /* localStorage unavailable — fall through and show the modal */
    }
    return new Promise<boolean>((resolve) =>
      set({ open: true, pluginId, command, _resolve: resolve }),
    );
  },
  confirm: () => {
    try {
      localStorage.setItem(seenKey(get().pluginId), "1");
    } catch {
      /* ignore persistence failure */
    }
    const resolve = get()._resolve;
    set({ open: false, _resolve: null });
    resolve?.(true);
  },
  cancel: () => {
    const resolve = get()._resolve;
    set({ open: false, _resolve: null });
    resolve?.(false);
  },
}));

/** Ask the user (once per plugin) before that plugin runs a program. */
export const requestSpawnConsent = (pluginId: string, command: string): Promise<boolean> =>
  useSpawnConsent.getState().request(pluginId, command);
