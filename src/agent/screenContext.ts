import type { Mode } from "../layout";
import type { View } from "../store";

export interface SelectedFlow {
  id: number;
  method: string;
  url: string;
  status: number | null;
}

/** What a plugin says about its own screen, asked for at send time. */
export interface PluginContext {
  pluginId: string;
  text: string;
}

/** One plugin's share of the block, and the block's own ceiling. Plugins see
 *  the agent's prompt only through this gap, and the temptation to pour
 *  everything into it is exactly why the limit truncates rather than advises. */
export const PLUGIN_CAP = 600;
export const PLUGINS_TOTAL_CAP = 2000;

export interface ScreenInput {
  mode: Mode;
  view: View;
  selected: SelectedFlow | null;
  flowCount: number;
  project?: { id: string; name: string } | null;
  /** When the traffic now on screen starts. Null before anything is captured. */
  sessionStartedMs?: number | null;
  /** Plugin-supplied lines, the active mode's plugin first so that it is the
   *  one that survives the ceiling. */
  plugins?: PluginContext[];
}

const SCREEN_NAMES: Record<string, string> = {
  setup: "Setup",
  settings: "Settings",
  plugins: "Plugins",
};

const VIEW_NAMES: Record<View, string> = {
  traffic: "Traffic capture",
  rules: "Rules",
  breakpoints: "Breakpoints",
};

const clip = (text: string, limit: number) =>
  text.length <= limit ? text : `${text.slice(0, limit)}… (truncated)`;

/** Plugin contributions, each clipped, and the whole set clipped again. */
function pluginLines(plugins: PluginContext[]): string[] {
  const out: string[] = [];
  let used = 0;
  for (const { pluginId, text } of plugins) {
    const body = clip(text.trim(), PLUGIN_CAP);
    if (!body) continue;
    if (used + body.length > PLUGINS_TOTAL_CAP) {
      out.push(`plugin ${pluginId}: (omitted — the screen block is full)`);
      continue;
    }
    used += body.length;
    out.push(`plugin ${pluginId}: ${body}`);
  }
  return out;
}

/**
 * A block prepended to every message. It carries pointers, never payloads:
 * the agent pulls what it needs through the MCP tools, where truncation and
 * UTF-8 boundaries are already handled.
 */
export function describeScreen(input: ScreenInput): string {
  const lines: string[] = [];
  const name =
    input.mode === "traffic" ? VIEW_NAMES[input.view] : SCREEN_NAMES[input.mode] ?? input.mode;
  lines.push(`screen: ${name}`);

  // Named even when absent: silence here would read as "scoped to something",
  // when in fact the answers would span every project captured.
  lines.push(
    input.project
      ? `project: ${input.project.name} (${input.project.id})`
      : "project: no project is active — traffic queries are not narrowed",
  );
  if (input.sessionStartedMs != null) {
    lines.push(`current session starts at: ${input.sessionStartedMs} (unix ms)`);
  }

  for (const line of pluginLines(input.plugins ?? [])) lines.push(line);

  if (input.mode === "traffic") {
    lines.push(`flows captured: ${input.flowCount}`);
    lines.push(
      input.selected
        ? `selected: flow id ${input.selected.id} — ${input.selected.method} ${input.selected.url}` +
            (input.selected.status === null ? "" : ` → ${input.selected.status}`)
        : "selected: nothing",
    );
  }
  return `<screen>\n${lines.join("\n")}\n</screen>`;
}
