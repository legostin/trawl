export type CoreTab = "overview" | "request" | "response" | "timing";
export type FlowTab = CoreTab | `plugin:${string}`;

const CORE: { value: CoreTab; label: string }[] = [
  { value: "overview", label: "Overview" },
  { value: "request", label: "Request" },
  { value: "response", label: "Response" },
  { value: "timing", label: "Timing" },
];

/** Core tabs first, then whatever plugins contributed. Plugin ids are
 *  namespaced so they can never collide with a core tab. */
export function flowTabs(
  panels: { id: string; label: string }[],
): { value: FlowTab; label: string }[] {
  return [...CORE, ...panels.map((p) => ({ value: `plugin:${p.id}` as FlowTab, label: p.label }))];
}

/** A plugin disabled while its tab was open must not leave a blank card. */
export function resolveTab(active: FlowTab, tabs: { value: FlowTab }[]): FlowTab {
  return tabs.some((t) => t.value === active) ? active : "overview";
}
