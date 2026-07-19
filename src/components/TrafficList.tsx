import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useFlows } from "../store";

export function TrafficList() {
  const flows = useFlows((s) => s.filteredFlows());
  const selectedId = useFlows((s) => s.selectedId);
  const select = useFlows((s) => s.select);
  const parentRef = useRef<HTMLDivElement>(null);

  const rowVirtualizer = useVirtualizer({
    count: flows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 28,
    overscan: 20,
  });

  return (
    <div ref={parentRef} style={{ height: "100%", overflow: "auto" }}>
      <div style={{ height: rowVirtualizer.getTotalSize(), position: "relative" }}>
        {rowVirtualizer.getVirtualItems().map((vi) => {
          const flow = flows[vi.index];
          const status = flow.response?.status ?? "";
          return (
            <div
              key={flow.id}
              onClick={() => select(flow.id)}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                height: vi.size,
                transform: `translateY(${vi.start}px)`,
                display: "flex",
                gap: 8,
                padding: "0 8px",
                fontSize: 12,
                lineHeight: `${vi.size}px`,
                cursor: "pointer",
                background:
                  flow.id === selectedId
                    ? "#2b4b6f"
                    : flow.state === "error"
                      ? "#5a1e1e"
                      : "transparent",
                whiteSpace: "nowrap",
                boxSizing: "border-box",
              }}
            >
              <span style={{ width: 50 }}>{flow.method}</span>
              <span style={{ width: 40 }}>{status}</span>
              <span style={{ width: 160, overflow: "hidden", textOverflow: "ellipsis" }}>
                {flow.url.host}
              </span>
              <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis" }}>
                {flow.url.path}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
