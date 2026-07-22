import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { load as loadYaml } from "js-yaml";
import { useLayout } from "./layout";
import { bus } from "./plugins/bus";
import type { FlowAction, RegisteredMode } from "./plugins/api";

/** An entry in the public plugin catalog (`plugins.yaml`). */
export interface CatalogEntry {
  id: string;
  name: string;
  description?: string;
  author?: string;
  /** "owner/repo". */
  repo: string;
  /** Git host; defaults to github.com. */
  host?: string;
  tags?: string[];
}

/** Fetch and parse the public plugin catalog (raw YAML fetched by the backend). */
export async function fetchCatalog(): Promise<CatalogEntry[]> {
  const text = await invoke<string>("fetch_plugin_catalog");
  const doc = loadYaml(text) as { plugins?: CatalogEntry[] } | null;
  return (doc?.plugins ?? []).filter((p) => p && p.id && p.repo);
}

/** The repo string to install a catalog entry (host-prefixed for non-github hosts). */
export function catalogInstallRepo(e: CatalogEntry): string {
  return e.host && e.host !== "github.com" ? `${e.host}/${e.repo}` : e.repo;
}

/** If a mode is being removed while it's active, fall back to the traffic mode. */
function leaveModeIfActive(id: string) {
  if (useLayout.getState().mode === id) useLayout.getState().setMode("traffic");
}

export interface Plugin {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  repo: string;
  /** Git host, e.g. "github.com" or a GitHub Enterprise domain. */
  host: string;
  ref: string;
  enabled: boolean;
}

export interface PluginDep {
  id: string;
  repo: string;
  host?: string;
  reference?: string;
  /** Reinstall the dependency when the installed version is older. */
  minVersion?: string;
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  entry: string;
  apiVersion: string;
  /** Plugins auto-installed alongside this one. */
  dependencies?: PluginDep[];
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
  /** Action buttons registered into the request-detail toolbar. */
  flowActions: FlowAction[];
  /** pluginId → newer version available in its repo (from the last check). */
  updates: Record<string, string>;
  load: () => Promise<void>;
  fetchManifest: (repo: string, reference?: string, host?: string) => Promise<PluginManifest>;
  install: (repo: string, reference?: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  setEnabled: (id: string, enabled: boolean) => Promise<void>;
  registerMode: (mode: RegisteredMode) => void;
  /** Add/replace an action button in the request-detail toolbar. */
  registerFlowAction: (action: FlowAction) => void;
  /** Remove a plugin's registered mode from the UI (hot disable). */
  unregisterMode: (id: string) => void;
  /** Fetch each installed plugin's manifest and record newer versions. */
  checkUpdates: () => Promise<void>;
  /** Re-fetch a plugin's bundle at its ref (applies on restart). */
  update: (id: string) => Promise<void>;
}

export const usePlugins = create<PluginsState>((set, get) => ({
  installed: [],
  modes: [],
  flowActions: [],
  updates: {},
  load: async () => set({ installed: await invoke<Plugin[]>("list_plugins") }),
  fetchManifest: (repo, reference, host) =>
    invoke<PluginManifest>("fetch_plugin_manifest", { repo, reference, host }),
  install: async (repo, reference) => {
    const before = new Set(get().installed.map((p) => p.id));
    const installed = await invoke<Plugin[]>("install_plugin", { repo, reference });
    set({ installed });
    const added = installed.find((p) => !before.has(p.id));
    if (added) {
      bus.emit("plugin:installed", { id: added.id, name: added.name, version: added.version });
    }
  },
  remove: async (id) => {
    const installed = await invoke<Plugin[]>("remove_plugin", { id });
    const updates = { ...get().updates };
    delete updates[id];
    leaveModeIfActive(id);
    set({ installed, updates, modes: get().modes.filter((m) => m.id !== id) });
    const { clearPluginTools } = await import("./plugins/mcpBridge");
    await clearPluginTools(id);
    bus.emit("plugin:removed", { id });
  },
  setEnabled: async (id, enabled) => {
    const installed = await invoke<Plugin[]>("set_plugin_enabled", { id, enabled });
    set({ installed });
    if (!enabled) {
      const { clearPluginTools } = await import("./plugins/mcpBridge");
      await clearPluginTools(id);
    }
  },
  registerMode: (mode) =>
    set((s) => ({
      modes: s.modes.some((m) => m.id === mode.id)
        ? s.modes.map((m) => (m.id === mode.id ? mode : m))
        : [...s.modes, mode],
    })),
  registerFlowAction: (action) =>
    set((s) => ({
      flowActions: s.flowActions.some((a) => a.id === action.id)
        ? s.flowActions.map((a) => (a.id === action.id ? action : a))
        : [...s.flowActions, action],
    })),
  unregisterMode: (id) => {
    leaveModeIfActive(id);
    set((s) => ({ modes: s.modes.filter((m) => m.id !== id) }));
  },
  checkUpdates: async () => {
    const found: Record<string, string> = {};
    await Promise.all(
      get().installed.map(async (p) => {
        try {
          const m = await invoke<PluginManifest>("fetch_plugin_manifest", {
            repo: p.repo,
            reference: p.ref,
            host: p.host,
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
    const installed = await invoke<Plugin[]>("install_plugin", {
      repo: p.repo,
      reference: p.ref,
      host: p.host,
    });
    const updates = { ...get().updates };
    delete updates[id];
    set({ installed, updates });
  },
}));
