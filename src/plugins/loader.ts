import { invoke } from "@tauri-apps/api/core";
import { usePlugins } from "@/plugins";

/** Load a cached plugin bundle by injecting it as a classic script (IIFE self-registers).
 *  Re-loading replaces any previous injection for the same plugin, so updates and
 *  enable/disable can be applied live (the re-run re-registers the mode). */
async function loadBundle(id: string): Promise<void> {
  const code = await invoke<string>("read_plugin_bundle", { id });
  const blob = new Blob([code], { type: "text/javascript" });
  const url = URL.createObjectURL(blob);
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
    URL.revokeObjectURL(url);
  }
}

/** Load every enabled plugin. Failures are logged but never block startup. */
export async function loadEnabledPlugins(): Promise<void> {
  await usePlugins.getState().load();
  const enabled = usePlugins.getState().installed.filter((p) => p.enabled);
  for (const p of enabled) {
    try {
      await loadBundle(p.id);
    } catch (e) {
      console.error("[trawl] plugin load failed:", p.id, e);
    }
  }
}

/** Load a single plugin on demand (e.g. right after install). */
export async function loadPlugin(id: string): Promise<void> {
  try {
    await loadBundle(id);
  } catch (e) {
    console.error("[trawl] plugin load failed:", id, e);
  }
}
