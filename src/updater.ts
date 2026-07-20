import { create } from "zustand";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { useToast } from "./toast";

export type UpdateStatus =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "error";

interface UpdaterState {
  status: UpdateStatus;
  version: string | null;
  notes: string | null;
  /** Download progress, 0–100. */
  progress: number;
  error: string | null;
  /** Handle returned by check(); kept to install the same update. */
  update: Update | null;
  /** Look for a newer release. `silent` suppresses the "up to date" / error toasts. */
  check: (silent: boolean) => Promise<void>;
  /** Download, install and relaunch into the available update. */
  install: () => Promise<void>;
}

export const useUpdater = create<UpdaterState>((set, get) => ({
  status: "idle",
  version: null,
  notes: null,
  progress: 0,
  error: null,
  update: null,

  check: async (silent) => {
    const s = get().status;
    if (s === "checking" || s === "downloading" || s === "ready") return;
    set({ status: "checking", error: null });
    try {
      const u = await check();
      if (u) {
        set({ status: "available", update: u, version: u.version, notes: u.body ?? null });
      } else {
        set({ status: "idle", update: null });
        if (!silent) useToast.getState().show("You're on the latest version");
      }
    } catch (e) {
      // In `tauri dev` the updater is unavailable — stay quiet on silent checks.
      set({ status: silent ? "idle" : "error", error: String(e) });
      if (!silent) useToast.getState().show("Update check failed");
    }
  },

  install: async () => {
    const u = get().update;
    if (!u) return;
    set({ status: "downloading", progress: 0, error: null });
    try {
      let total = 0;
      let downloaded = 0;
      await u.downloadAndInstall((ev) => {
        switch (ev.event) {
          case "Started":
            total = ev.data.contentLength ?? 0;
            break;
          case "Progress":
            downloaded += ev.data.chunkLength;
            if (total > 0) set({ progress: Math.round((downloaded / total) * 100) });
            break;
          case "Finished":
            set({ progress: 100, status: "ready" });
            break;
        }
      });
      set({ status: "ready" });
      await relaunch();
    } catch (e) {
      set({ status: "error", error: String(e) });
      useToast.getState().show("Update failed");
    }
  },
}));
