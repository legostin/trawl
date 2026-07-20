import { useMemo, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowRight, Inbox, SearchX, Wand2 } from "lucide-react";
import { useFlows } from "../store";
import { visibleFlows } from "../filter";
import { MethodBadge, StatusBadge } from "./badges";
import { EmptyState } from "./EmptyState";
import { bodyLength, formatBytes, formatClock } from "@/lib/format";
import { cn } from "@/lib/utils";

const COLS = "20px 52px 48px minmax(110px,1.3fr) minmax(120px,2fr) 64px 76px";

export function TrafficTable() {
  const allFlows = useFlows((s) => s.flows);
  const filter = useFlows((s) => s.filter);
  const flows = useMemo(() => visibleFlows(allFlows, filter), [allFlows, filter]);
  const selectedId = useFlows((s) => s.selectedId);
  const select = useFlows((s) => s.select);
  const parentRef = useRef<HTMLDivElement>(null);

  const rowVirtualizer = useVirtualizer({
    count: flows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 30,
    overscan: 20,
  });

  return (
    <div className="flex h-full flex-col">
      <div
        className="grid items-center gap-2 border-b border-border bg-card px-3 py-1.5 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground"
        style={{ gridTemplateColumns: COLS }}
      >
        <span />
        <span>Method</span>
        <span>Code</span>
        <span>Host</span>
        <span>Path</span>
        <span className="text-right">Size</span>
        <span className="text-right">Time</span>
      </div>

      {flows.length === 0 ? (
        allFlows.length === 0 ? (
          <EmptyState
            icon={<Inbox className="size-8" />}
            title="No traffic yet"
            hint="Press Start and route requests through the proxy at 0.0.0.0:8729."
          />
        ) : (
          <EmptyState
            icon={<SearchX className="size-8" />}
            title="Nothing found"
            hint="Try changing the search or filters."
          />
        )
      ) : (
        <div ref={parentRef} className="min-h-0 flex-1 overflow-auto">
          <div style={{ height: rowVirtualizer.getTotalSize(), position: "relative" }}>
            {rowVirtualizer.getVirtualItems().map((vi) => {
              const flow = flows[vi.index];
              const size = bodyLength(flow.response);
              const selected = flow.id === selectedId;
              return (
                <div
                  key={flow.id}
                  onClick={() => select(flow.id)}
                  className={cn(
                    "absolute left-0 top-0 grid w-full cursor-pointer items-center gap-2 border-b border-border/50 px-3 text-xs",
                    selected
                      ? "bg-primary/15"
                      : flow.state === "error"
                        ? "bg-http-red/10 hover:bg-http-red/15"
                        : vi.index % 2
                          ? "bg-muted/30 hover:bg-accent"
                          : "hover:bg-accent",
                  )}
                  style={{
                    height: vi.size,
                    transform: `translateY(${vi.start}px)`,
                    gridTemplateColumns: COLS,
                  }}
                >
                  {flow.appliedRules.length > 0 ? (
                    <span title={`Rule applied: ${flow.appliedRules.join(", ")}`}>
                      <Wand2 className="size-3 text-http-amber" />
                    </span>
                  ) : (
                    <span title="Proxied (no rule)">
                      <ArrowRight className="size-3 text-muted-foreground/40" />
                    </span>
                  )}
                  <MethodBadge method={flow.method} />
                  <StatusBadge status={flow.response?.status} />
                  <span className="truncate text-foreground">{flow.url.host}</span>
                  <span className="truncate text-muted-foreground">{flow.url.path}</span>
                  <span className="text-right font-mono text-[11px] text-muted-foreground">
                    {formatBytes(size)}
                  </span>
                  <span className="text-right font-mono text-[11px] text-muted-foreground">
                    {formatClock(flow.timestamp)}
                  </span>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
