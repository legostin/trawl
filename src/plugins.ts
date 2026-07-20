import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { RegisteredMode } from "./plugins/api";

export interface Plugin {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  repo: string;
  ref: string;
  enabled: boolean;
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  entry: string;
  apiVersion: string;
}

/** Compare dotted numeric versions. Returns >0 if a is newer than b. */
export function cmpVersions(a: string, b: string): number {
  const pa = a.split(".").map((n) => parseInt(n, 10) || 0);
  const pb = b.split(".").map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

interface PluginsState {
  /** Installed plugins (from the on-disk registry). */
  installed: Plugin[];
  /** Modes registered by loaded plugins at runtime. */
  modes: RegisteredMode[];
  /** pluginId → newer version available in its repo (from the last check). */
  updates: Record<string, string>;
  load: () => Promise<void>;
  fetchManifest: (repo: string, reference?: string) => Promise<PluginManifest>;
  install: (repo: string, reference?: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  setEnabled: (id: string, enabled: boolean) => Promise<void>;
  registerMode: (mode: RegisteredMode) => void;
  /** Fetch each installed plugin's manifest and record newer versions. */
  checkUpdates: () => Promise<void>;
  /** Re-fetch a plugin's bundle at its ref (applies on restart). */
  update: (id: string) => Promise<void>;
}

export const usePlugins = create<PluginsState>((set, get) => ({
  installed: [],
  modes: [],
  updates: {},
  load: async () => set({ installed: await invoke<Plugin[]>("list_plugins") }),
  fetchManifest: (repo, reference) =>
    invoke<PluginManifest>("fetch_plugin_manifest", { repo, reference }),
  install: async (repo, reference) => {
    const installed = await invoke<Plugin[]>("install_plugin", { repo, reference });
    set({ installed });
  },
  remove: async (id) => {
    const installed = await invoke<Plugin[]>("remove_plugin", { id });
    const updates = { ...get().updates };
    delete updates[id];
    set({ installed, updates, modes: get().modes.filter((m) => m.id !== id) });
  },
  setEnabled: async (id, enabled) => {
    const installed = await invoke<Plugin[]>("set_plugin_enabled", { id, enabled });
    set({ installed });
  },
  registerMode: (mode) =>
    set((s) => ({
      modes: s.modes.some((m) => m.id === mode.id)
        ? s.modes.map((m) => (m.id === mode.id ? mode : m))
        : [...s.modes, mode],
    })),
  checkUpdates: async () => {
    const found: Record<string, string> = {};
    await Promise.all(
      get().installed.map(async (p) => {
        try {
          const m = await invoke<PluginManifest>("fetch_plugin_manifest", {
            repo: p.repo,
            reference: p.ref,
          });
          if (cmpVersions(m.version, p.version) > 0) found[p.id] = m.version;
        } catch {
          /* offline / manifest gone — skip */
        }
      }),
    );
    set({ updates: found });
  },
  update: async (id) => {
    const p = get().installed.find((x) => x.id === id);
    if (!p) return;
    const installed = await invoke<Plugin[]>("install_plugin", { repo: p.repo, reference: p.ref });
    const updates = { ...get().updates };
    delete updates[id];
    set({ installed, updates });
  },
}));
