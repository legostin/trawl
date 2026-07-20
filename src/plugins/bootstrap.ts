import { installHost } from "./host";
import { loadEnabledPlugins } from "./loader";
import { usePlugins } from "@/plugins";

let started = false;

/** Install the host API, load enabled plugins, then check for plugin updates. Idempotent. */
export async function bootstrapPlugins(): Promise<void> {
  if (started) return;
  started = true;
  installHost();
  await loadEnabledPlugins();
  // Automatic update check (non-blocking; failures are ignored).
  void usePlugins.getState().checkUpdates();
}
