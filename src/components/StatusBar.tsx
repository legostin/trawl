import { useMemo } from "react";
import { useFlows } from "../store";
import { flowMatches } from "../filter";
import { cn } from "@/lib/utils";

export function StatusBar() {
  const running = useFlows((s) => s.running);
  const proxyAddr = useFlows((s) => s.proxyAddr);
  const flows = useFlows((s) => s.flows);
  const filter = useFlows((s) => s.filter);
  const shown = useMemo(
    () => flows.filter((f) => flowMatches(f, filter)).length,
    [flows, filter],
  );

  return (
    <footer className="flex h-6 items-center gap-3 border-t border-border bg-card px-3 text-[11px] text-muted-foreground">
      <span className="flex items-center gap-1.5">
        <span
          className={cn(
            "size-2 rounded-full",
            running ? "bg-http-green" : "bg-http-gray",
          )}
        />
        {running ? "running" : "stopped"}
      </span>
      {proxyAddr && <span className="font-mono">{proxyAddr}</span>}
      <div className="ml-auto flex items-center gap-3">
        <span>
          flows <span className="text-foreground">{flows.length}</span>
        </span>
        <span>
          shown <span className="text-foreground">{shown}</span>
        </span>
        <span className="opacity-60">↑↓ навигация · / поиск</span>
      </div>
    </footer>
  );
}
