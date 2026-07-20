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

interface PluginsState {
  /** Installed plugins (from the on-disk registry). */
  installed: Plugin[];
  /** Modes registered by loaded plugins at runtime. */
  modes: RegisteredMode[];
  load: () => Promise<void>;
  fetchManifest: (repo: string, reference?: string) => Promise<PluginManifest>;
  install: (repo: string, reference?: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  setEnabled: (id: string, enabled: boolean) => Promise<void>;
  registerMode: (mode: RegisteredMode) => void;
}

export const usePlugins = create<PluginsState>((set, get) => ({
  installed: [],
  modes: [],
  load: async () => set({ installed: await invoke<Plugin[]>("list_plugins") }),
  fetchManifest: (repo, reference) =>
    invoke<PluginManifest>("fetch_plugin_manifest", { repo, reference }),
  install: async (repo, reference) => {
    const installed = await invoke<Plugin[]>("install_plugin", { repo, reference });
    set({ installed });
  },
  remove: async (id) => {
    const installed = await invoke<Plugin[]>("remove_plugin", { id });
    set({ installed, modes: get().modes.filter((m) => m.id !== id) });
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
}));
