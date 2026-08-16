import type { Mode } from "../layout";
import type { View } from "../store";

export interface SelectedFlow {
  id: number;
  method: string;
  url: string;
  status: number | null;
}

export interface ScreenInput {
  mode: Mode;
  view: View;
  selected: SelectedFlow | null;
  flowCount: number;
  project?: { id: string; name: string } | null;
  /** When the traffic now on screen starts. Null before anything is captured. */
  sessionStartedMs?: number | null;
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
