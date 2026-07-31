import { listen } from "@tauri-apps/api/event";
import { useRules } from "./rules";
import { useProjects } from "./projects";
import { useBreakpoints } from "./breakpoints";

/** What the backend says has changed underneath the window. */
export type Changed = "rules" | "projects" | "breakpoints";

const RELOAD: Record<Changed, () => Promise<void>> = {
  rules: () => useRules.getState().load(),
  projects: () => useProjects.getState().load(),
  breakpoints: () => useBreakpoints.getState().load(),
};

/** Reload the part of the state the backend says has changed. */
export function applyChange(what: string): void {
  const reload = RELOAD[what as Changed];
  if (reload) void reload();
}

/**
 * The window holds its own copy of rules, projects and breakpoints, so anything
 * that changes them elsewhere — an MCP tool called from a chat, most of all —
 * is invisible until something happens to reload that copy. A rule created from
 * a chat and missing from the list reads as "MCP did not work".
 */
export function watchExternalChanges(): void {
  void listen<{ what?: string }>("state-changed", (event) => {
    if (event.payload?.what) applyChange(event.payload.what);
  });
}
