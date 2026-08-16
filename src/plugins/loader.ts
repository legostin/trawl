import { invoke } from "@tauri-apps/api/core";
import { apiCompatible, HOST_API_VERSION, usePlugins, type Plugin } from "@/plugins";
import { useToast } from "@/toast";
import { clearPluginTools, setLoadingPlugin } from "./mcpBridge";
import { makeHost } from "./host";
import { killPluginProcesses } from "./procApi";

/** True when the cached bundle can run on this app; otherwise warn instead of
 *  executing it (an incompatible bundle would crash at render with a cryptic
 *  React error — see plugins installed before the apiVersion gate existed). */
function guardApiVersion(p: Plugin): boolean {
  if (apiCompatible(p.apiVersion)) return true;
  console.error(
    `[trawl] plugin "${p.id}" needs plugin API ${p.apiVersion}, app provides ${HOST_API_VERSION} — skipped`,
  );
  useToast
    .getState()
    .show(`Plugin "${p.name}" needs a newer app version — update the app to use it`);
  return false;
}

/** Load a cached plugin bundle by injecting it as a classic script (IIFE self-registers).
 *  Re-loading replaces any previous injection for the same plugin, so updates and
 *  enable/disable can be applied live (the re-run re-registers the mode). */
async function loadBundle(id: string): Promise<void> {
  // `setLoadingPlugin` and `window.__TRAWL__` are single module-level slots, so
  // two overlapping loads would attribute one plugin's MCP tools to the other
  // and hand a bundle the wrong host. User-paced clicks never collide; two
  // agent writes one sentence apart do.
  const run = chain.then(() => injectBundle(id));
  chain = run.catch(() => {});
  return run;
}

let chain: Promise<void> = Promise.resolve();

async function injectBundle(id: string): Promise<void> {
  const code = await invoke<string>("read_plugin_bundle", { id });
  await clearPluginTools(id);
  // A reload replaces the bundle, so anything the previous injection started
  // would otherwise keep running unowned.
  await killPluginProcesses(id).catch(() => {});
  const blob = new Blob([code], { type: "text/javascript" });
  const url = URL.createObjectURL(blob);
  setLoadingPlugin(id);
  // The bundle captures `window.__TRAWL__` at init; hand it a host that knows
  // which plugin it belongs to.
  window.__TRAWL__ = makeHost(id);
  try {
    document
      .querySelectorAll(`script[data-trawl-plugin="${CSS.escape(id)}"]`)
      .forEach((s) => s.remove());
    await new Promise<void>((resolve, reject) => {
      const script = document.createElement("script");
      script.src = url;
      script.dataset.trawlPlugin = id;
      script.onload = () => resolve();
      script.onerror = () => reject(new Error(`failed to execute plugin bundle "${id}"`));
      document.head.appendChild(script);
    });
  } finally {
    setLoadingPlugin(null);
    // Back to an unowned host: a late `window.__TRAWL__` read gets an object
    // whose process API refuses to spawn rather than one attributed to whoever
    // happened to load last.
    window.__TRAWL__ = makeHost(null);
    URL.revokeObjectURL(url);
  }
}

/** Load every enabled plugin. Failures are logged but never block startup. */
export async function loadEnabledPlugins(): Promise<void> {
  await usePlugins.getState().load();
  const enabled = usePlugins.getState().installed.filter((p) => p.enabled);
  for (const p of enabled) {
    if (!guardApiVersion(p)) continue;
    try {
      await loadBundle(p.id);
    } catch (e) {
      console.error("[trawl] plugin load failed:", p.id, e);
    }
  }
}

/** Load a single plugin on demand (e.g. right after install). */
export async function loadPlugin(id: string): Promise<void> {
  const p = usePlugins.getState().installed.find((x) => x.id === id);
  if (p && !guardApiVersion(p)) return;
  // Called from a backend event as well as from the panel's buttons, and a
  // disabled plugin must stay disabled however the call arrived.
  if (p && !p.enabled) return;
  try {
    await loadBundle(id);
  } catch (e) {
    console.error("[trawl] plugin load failed:", id, e);
  }
}
