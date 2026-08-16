import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { ProcessInfo, ProcessLine, TrawlDialog, TrawlProcess } from "./api";

interface RawProcessInfo {
  id: string;
  pid: number;
  plugin_id: string;
  command: string;
  started_at: number;
}

const toInfo = (raw: RawProcessInfo): ProcessInfo => ({
  id: raw.id,
  pid: raw.pid,
  pluginId: raw.plugin_id,
  command: raw.command,
  startedAt: raw.started_at,
});

/** Native dialogs. Shared by every plugin — nothing here is per-plugin state. */
export const dialogApi = (): TrawlDialog => ({
  pickFolder: async (options) => {
    const picked = await openDialog({
      directory: true,
      multiple: false,
      title: options?.title,
      defaultPath: options?.defaultPath,
    });
    return typeof picked === "string" ? picked : null;
  },
  pickFile: async (options) => {
    const picked = await openDialog({
      directory: false,
      multiple: false,
      title: options?.title,
      defaultPath: options?.defaultPath,
      filters: options?.filters,
    });
    return typeof picked === "string" ? picked : null;
  },
});

/**
 * Child processes bound to one plugin. The plugin id comes from the loader,
 * which hands each bundle its own host object — that is how a spawn made later,
 * on a click, is still attributed to the right plugin.
 */
export const processApi = (pluginId: string | null): TrawlProcess => {
  const owner = (): string => {
    if (!pluginId) {
      throw new Error(
        "host.process is only available on the host object captured during plugin initialization",
      );
    }
    return pluginId;
  };

  return {
    spawn: async (request) => {
      const id = owner();
      // Consent is asked for by the backend, in a window the OS owns. Asking
      // here as well would double the prompt, and a check living in the same
      // page as the plugin is one the plugin can answer for itself.
      const raw = await invoke<RawProcessInfo>("plugin_spawn", { pluginId: id, request });
      return toInfo(raw);
    },
    onOutput: (id, cb) => {
      let stop = () => {};
      void listen<{ id: string; stream: ProcessLine["stream"]; text: string }>(
        "plugin-process-output",
        (e) => {
          if (e.payload.id === id) cb({ stream: e.payload.stream, text: e.payload.text });
        },
      ).then((un) => {
        stop = un;
      });
      return () => stop();
    },
    onExit: (id, cb) => {
      let stop = () => {};
      void listen<{ id: string; code: number | null }>("plugin-process-exit", (e) => {
        if (e.payload.id === id) cb({ code: e.payload.code });
      }).then((un) => {
        stop = un;
      });
      return () => stop();
    },
    kill: (id) => invoke<void>("plugin_kill_process", { id }),
    list: async () =>
      (await invoke<RawProcessInfo[]>("plugin_list_processes", { pluginId: owner() })).map(toInfo),
  };
};

/** Stop everything a plugin started (disable, reload, uninstall). */
export const killPluginProcesses = (pluginId: string): Promise<void> =>
  invoke<void>("plugin_kill_processes", { pluginId });
