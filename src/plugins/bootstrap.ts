import { installHost } from "./host";
import { loadEnabledPlugins } from "./loader";
import { usePlugins } from "@/plugins";

let started = false;

const SAFE_MODE_KEY = "trawl-skip-plugins";

/**
 * Whether this launch should skip plugins entirely.
 *
 * A plugin that loops or throws at init runs before the window is usable, so
 * there is no Plugins panel left to remove it with and a restart runs the same
 * bundle again. Without an escape the only way out is editing plugins.json by
 * hand. Arming this from the console — `localStorage["trawl-skip-plugins"]=1`
 * — is enough, because a frozen page is the one case where the console still
 * works and the UI does not. The flag clears itself, so the next launch is
 * normal again.
 */
function takeSafeMode(): boolean {
  try {
    if (!localStorage.getItem(SAFE_MODE_KEY)) return false;
    localStorage.removeItem(SAFE_MODE_KEY);
    return true;
  } catch {
    return false;
  }
}

/** Install the host API, load enabled plugins, then check for plugin updates. Idempotent. */
export async function bootstrapPlugins(): Promise<void> {
  if (started) return;
  started = true;
  installHost();
  if (takeSafeMode()) {
    console.warn("[trawl] safe mode: plugins were not loaded this launch");
    await usePlugins.getState().load();
    return;
  }
  await loadEnabledPlugins();
  // Automatic update check (non-blocking; failures are ignored).
  void usePlugins.getState().checkUpdates();
}
