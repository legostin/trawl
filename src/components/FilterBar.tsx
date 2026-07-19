import { Filter, X } from "lucide-react";
import { useFlows } from "../store";
import type { StatusClass } from "../filter";
import { Select } from "./ui/select";
import { Button } from "./ui/button";

const METHODS = ["", "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
const STATUS_CLASSES: StatusClass[] = ["any", "2xx", "3xx", "4xx", "5xx"];

export function FilterBar() {
  const filter = useFlows((s) => s.filter);
  const setFilter = useFlows((s) => s.setFilter);
  const clearFilter = useFlows((s) => s.clearFilter);

  const active = filter.method !== "" || filter.statusClass !== "any" || filter.query !== "";

  return (
    <div className="flex items-center gap-2 border-b border-border bg-card/50 px-2 py-1.5">
      <Filter className="size-3.5 text-muted-foreground" />
      <Select value={filter.method} onChange={(e) => setFilter({ method: e.target.value })}>
        {METHODS.map((m) => (
          <option key={m} value={m}>
            {m === "" ? "method: any" : m}
          </option>
        ))}
      </Select>
      <Select
        value={filter.statusClass}
        onChange={(e) => setFilter({ statusClass: e.target.value as StatusClass })}
      >
        {STATUS_CLASSES.map((c) => (
          <option key={c} value={c}>
            {c === "any" ? "status: any" : c}
          </option>
        ))}
      </Select>
      {active && (
        <Button variant="ghost" size="sm" className="ml-auto" onClick={clearFilter}>
          <X />
          Reset
        </Button>
      )}
    </div>
  );
}
