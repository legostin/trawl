import { listen } from "@tauri-apps/api/event";
import { useRules } from "./rules";
import { useProjects } from "./projects";
import { useBreakpoints } from "./breakpoints";

/** What the backend says has changed underneath the window. */
export type Changed = "rules" | "projects" | "breakpoints" | "plugins";

const RELOAD: Record<Changed, (id?: string) => Promise<void>> = {
  rules: () => useRules.getState().load(),
  projects: () => useProjects.getState().load(),
  breakpoints: () => useBreakpoints.getState().load(),
  // Imported lazily: this module is loaded from main.tsx, and reaching the
  // plugin loader statically would drag the whole UI graph in with it.
  plugins: async (id) => {
    const { usePlugins, forgetPlugin } = await import("./plugins");
    await usePlugins.getState().load();
    const { loadPlugin, loadEnabledPlugins } = await import("./plugins/loader");
    if (!id) {
      await loadEnabledPlugins();
      return;
    }
    const plugin = usePlugins.getState().installed.find((p) => p.id === id);
    if (!plugin) {
      // Gone from the registry means it was deleted; nothing to infer from a
      // second payload field.
      await forgetPlugin(id);
      return;
    }
    if (plugin.enabled) await loadPlugin(id);
  },
};

/** Reload the part of the state the backend says has changed. */
export function applyChange(what: string, id?: string): void {
  const reload = RELOAD[what as Changed];
  if (reload) void reload(id);
}

/**
 * The window holds its own copy of rules, projects and breakpoints, so anything
 * that changes them elsewhere — an MCP tool called from a chat, most of all —
 * is invisible until something happens to reload that copy. A rule created from
 * a chat and missing from the list reads as "MCP did not work".
 */
export function watchExternalChanges(): void {
  void listen<{ what?: string; id?: string }>("state-changed", (event) => {
    if (event.payload?.what) applyChange(event.payload.what, event.payload.id);
  });
}
