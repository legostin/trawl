import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { load as loadYaml } from "js-yaml";
import { useLayout } from "./layout";
import { bus } from "./plugins/bus";
import {
  HOST_API_VERSION,
  type FlowAction,
  type FlowPanel,
  type RegisteredMode,
} from "./plugins/api";

export { HOST_API_VERSION };

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
  /** Plugin API version the installed bundle needs (from its manifest; empty or
   *  missing for plugins installed before the registry recorded it). */
  apiVersion?: string;
  /** Where it came from. "local" was written inside the app and has no repo,
   *  so it never checks for updates. Missing in older registries = "git". */
  origin?: "git" | "local";
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

/** Whether a plugin's required API version is satisfied by this app. Missing or
 *  empty means the plugin predates the gate and is assumed compatible. */
export function apiCompatible(apiVersion: string | undefined): boolean {
  return !apiVersion || cmpVersions(apiVersion, HOST_API_VERSION) <= 0;
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
  /** Bumped whenever a bundle registers a mode. Error boundaries key off it, so
   *  a plugin that has been fixed stops showing its previous crash. */
  reloads: number;
  /** Action buttons registered into the request-detail toolbar. */
  flowActions: FlowAction[];
  /** Panels registered into the request-detail card, one tab each. */
  flowPanels: FlowPanel[];
  /** pluginId → newer version available in its repo (from the last check). */
  updates: Record<string, string>;
  /** pluginId → newer version that this app can't run yet (needs a newer host API). */
  blockedUpdates: Record<string, { version: string; apiVersion: string }>;
  load: () => Promise<void>;
  fetchManifest: (repo: string, reference?: string, host?: string) => Promise<PluginManifest>;
  install: (repo: string, reference?: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  setEnabled: (id: string, enabled: boolean) => Promise<void>;
  registerMode: (mode: RegisteredMode) => void;
  /** Add/replace an action button in the request-detail toolbar. */
  registerFlowAction: (action: FlowAction) => void;
  /** Add/replace a panel in the request-detail card. */
  registerFlowPanel: (panel: FlowPanel) => void;
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
  reloads: 0,
  flowActions: [],
  flowPanels: [],
  updates: {},
  blockedUpdates: {},
  load: async () => set({ installed: await invoke<Plugin[]>("list_plugins") }),
  fetchManifest: (repo, reference, host) =>
    invoke<PluginManifest>("fetch_plugin_manifest", { repo, reference, host }),
  install: async (repo, reference) => {
    const before = new Set(get().installed.map((p) => p.id));
    const installed = await invoke<Plugin[]>("install_plugin", {
      repo,
      reference,
      hostApiVersion: HOST_API_VERSION,
    });
    set({ installed });
    // A manifest with dependencies can install several new plugins at once.
    for (const added of installed.filter((p) => !before.has(p.id))) {
      bus.emit("plugin:installed", { id: added.id, name: added.name, version: added.version });
    }
  },
  remove: async (id) => {
    const installed = await invoke<Plugin[]>("remove_plugin", { id });
    set({ installed });
    await forgetPlugin(id);
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
      reloads: s.reloads + 1,
      modes: s.modes.some((m) => m.id === mode.id)
        ? s.modes.map((m) => (m.id === mode.id ? mode : m))
        : [...s.modes, mode],
    })),
  registerFlowPanel: (panel) =>
    set((s) => ({
      flowPanels: s.flowPanels.some((p) => p.id === panel.id)
        ? s.flowPanels.map((p) => (p.id === panel.id ? panel : p))
        : [...s.flowPanels, panel],
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
    const blocked: Record<string, { version: string; apiVersion: string }> = {};
    await Promise.all(
      // A plugin written in-app has no repo. Asking about one anyway fetches a
      // URL that cannot exist, which falls through to the authenticated path
      // and reads the Keychain — a prompt on every launch, which is exactly
      // what the unauthenticated-first fetch was built to avoid.
      get().installed.filter((p) => p.origin !== "local" && p.repo).map(async (p) => {
        try {
          const m = await invoke<PluginManifest>("fetch_plugin_manifest", {
            repo: p.repo,
            reference: p.ref,
            host: p.host,
          });
          if (cmpVersions(m.version, p.version) <= 0) return;
          if (apiCompatible(m.apiVersion)) found[p.id] = m.version;
          else blocked[p.id] = { version: m.version, apiVersion: m.apiVersion };
        } catch {
          /* offline / manifest gone — skip */
        }
      }),
    );
    set({ updates: found, blockedUpdates: blocked });
  },
  update: async (id) => {
    const p = get().installed.find((x) => x.id === id);
    if (!p) return;
    const installed = await invoke<Plugin[]>("install_plugin", {
      repo: p.repo,
      reference: p.ref,
      host: p.host,
      hostApiVersion: HOST_API_VERSION,
    });
    const updates = { ...get().updates };
    delete updates[id];
    set({ installed, updates });
  },
}));

/**
 * Undoes everything a loaded plugin left behind. Called both when the user
 * removes one and when a plugin disappears from the registry underneath us,
 * so "removed" means the same thing either way.
 *
 * The injected script and the flow actions were previously left in place —
 * uninstalling stopped the mode from appearing but not the plugin from running.
 */
export async function forgetPlugin(id: string): Promise<void> {
  leaveModeIfActive(id);
  usePlugins.setState((s) => {
    const updates = { ...s.updates };
    delete updates[id];
    const blockedUpdates = { ...s.blockedUpdates };
    delete blockedUpdates[id];
    return {
      updates,
      blockedUpdates,
      modes: s.modes.filter((m) => m.id !== id),
      flowActions: s.flowActions.filter((a) => a.pluginId !== id),
      flowPanels: s.flowPanels.filter((p) => p.pluginId !== id),
    };
  });
  const { clearPluginTools } = await import("./plugins/mcpBridge");
  await clearPluginTools(id);
  const { killPluginProcesses } = await import("./plugins/procApi");
  await killPluginProcesses(id).catch(() => {});
  if (typeof document !== "undefined") {
    document
      .querySelectorAll(`script[data-trawl-plugin="${CSS.escape(id)}"]`)
      .forEach((s) => s.remove());
  }
  bus.emit("plugin:removed", { id });
}
