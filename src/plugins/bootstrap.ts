import { installHost } from "./host";
import { loadEnabledPlugins } from "./loader";

let started = false;

/** Install the host API, then load all enabled plugins. Idempotent. */
export async function bootstrapPlugins(): Promise<void> {
  if (started) return;
  started = true;
  installHost();
  await loadEnabledPlugins();
}
