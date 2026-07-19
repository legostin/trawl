import { useFlows } from "../store";
import type { StatusClass } from "../filter";

const METHODS = ["", "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
const STATUS_CLASSES: StatusClass[] = ["any", "2xx", "3xx", "4xx", "5xx"];

export function FilterBar() {
  const filter = useFlows((s) => s.filter);
  const setFilter = useFlows((s) => s.setFilter);
  const total = useFlows((s) => s.flows.length);
  const shown = useFlows((s) => s.filteredFlows().length);

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "6px 8px",
        borderBottom: "1px solid #333",
        fontSize: 12,
      }}
    >
      <input
        value={filter.query}
        onChange={(e) => setFilter({ query: e.target.value })}
        placeholder="Поиск по host/URL…"
        style={{
          flex: 1,
          background: "#2a2a2a",
          color: "#ddd",
          border: "1px solid #444",
          padding: "3px 6px",
        }}
      />
      <select
        value={filter.method}
        onChange={(e) => setFilter({ method: e.target.value })}
        style={{ background: "#2a2a2a", color: "#ddd" }}
      >
        {METHODS.map((m) => (
          <option key={m} value={m}>
            {m === "" ? "метод: любой" : m}
          </option>
        ))}
      </select>
      <select
        value={filter.statusClass}
        onChange={(e) => setFilter({ statusClass: e.target.value as StatusClass })}
        style={{ background: "#2a2a2a", color: "#ddd" }}
      >
        {STATUS_CLASSES.map((c) => (
          <option key={c} value={c}>
            {c === "any" ? "статус: любой" : c}
          </option>
        ))}
      </select>
      <span style={{ opacity: 0.7, whiteSpace: "nowrap" }}>
        {shown} / {total}
      </span>
    </div>
  );
}
