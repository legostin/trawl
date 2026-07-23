import { create } from "zustand";

const SEEN_KEY = "trawl.keychainConsentSeen";

interface KeychainConsentState {
  open: boolean;
  _resolve: ((granted: boolean) => void) | null;
  /** Show the one-time explainer if not seen; resolves true to proceed with the
   *  Keychain-writing action, false if the user cancels. Resolves true instantly
   *  once the user has acknowledged it before. */
  request: () => Promise<boolean>;
  confirm: () => void;
  cancel: () => void;
}

export const useKeychainConsent = create<KeychainConsentState>((set, get) => ({
  open: false,
  _resolve: null,
  request: () => {
    try {
      if (localStorage.getItem(SEEN_KEY)) return Promise.resolve(true);
    } catch {
      /* localStorage unavailable — fall through and show the modal */
    }
    return new Promise<boolean>((resolve) => set({ open: true, _resolve: resolve }));
  },
  confirm: () => {
    try {
      localStorage.setItem(SEEN_KEY, "1");
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

/** Ask the user (once) before an action that reads/writes the macOS Keychain. */
export const requestKeychainConsent = (): Promise<boolean> => useKeychainConsent.getState().request();
