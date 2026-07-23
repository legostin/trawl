import type { Flow } from "./types";

export type StatusClass = "any" | "2xx" | "3xx" | "4xx" | "5xx";

export interface FlowFilter {
  query: string;
  method: string; // "" = any
  statusClass: StatusClass;
}

export const emptyFilter: FlowFilter = { query: "", method: "", statusClass: "any" };

function matchesStatusClass(status: number | undefined, cls: StatusClass): boolean {
  if (cls === "any") return true;
  if (status === undefined) return false;
  const bucket = Math.floor(status / 100);
  return `${bucket}xx` === cls;
}

/** Filtered flows, newest first (descending by id). */
export function visibleFlows(flows: Flow[], filter: FlowFilter): Flow[] {
  return flows.filter((f) => flowMatches(f, filter)).reverse();
}

export function flowMatches(flow: Flow, filter: FlowFilter): boolean {
  if (filter.method && flow.method !== filter.method) return false;

  if (!matchesStatusClass(flow.response?.status, filter.statusClass)) return false;

  const q = filter.query.trim().toLowerCase();
  if (q) {
    const haystack = `${flow.url.host}${flow.url.path}`.toLowerCase();
    if (!haystack.includes(q)) return false;
  }

  return true;
}
