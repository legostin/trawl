import { useMemo, useState } from "react";
import { ChevronRight, Inbox, SearchX } from "lucide-react";
import { useFlows } from "../store";
import { flowMatches } from "../filter";
import { buildDomainTree, type TreeBranch, type TreeLeaf } from "../tree";
import { MethodBadge, StatusBadge } from "./badges";
import { EmptyState } from "./EmptyState";
import { cn } from "@/lib/utils";

export function StructureTree() {
  const allFlows = useFlows((s) => s.flows);
  const filter = useFlows((s) => s.filter);
  const flows = useMemo(() => allFlows.filter((f) => flowMatches(f, filter)), [allFlows, filter]);
  const tree = useMemo(() => buildDomainTree(flows), [flows]);

  const selectedId = useFlows((s) => s.selectedId);
  const select = useFlows((s) => s.select);
  const [overrides, setOverrides] = useState<Record<string, boolean>>({});

  const isOpen = (key: string, depth: number) =>
    key in overrides ? overrides[key] : depth === 0;
  const toggle = (key: string, depth: number) =>
    setOverrides((o) => ({ ...o, [key]: !isOpen(key, depth) }));

  if (flows.length === 0) {
    return allFlows.length === 0 ? (
      <EmptyState
        icon={<Inbox className="size-8" />}
        title="No traffic yet"
        hint="Press Start and route requests through the proxy at 0.0.0.0:8888."
      />
    ) : (
      <EmptyState
        icon={<SearchX className="size-8" />}
        title="Nothing found"
        hint="Try changing the search or filters."
      />
    );
  }

  const renderBranch = (node: TreeBranch, depth: number) => {
    const open = isOpen(node.key, depth);
    return (
      <div key={node.key}>
        <div
          onClick={() => toggle(node.key, depth)}
          className="flex cursor-pointer items-center gap-1.5 py-1 pr-2 text-xs hover:bg-accent"
          style={{ paddingLeft: 8 + depth * 14 }}
        >
          <ChevronRight
            className={cn(
              "size-3.5 shrink-0 text-muted-foreground transition-transform",
              open && "rotate-90",
            )}
          />
          <span className="truncate font-medium text-foreground">{node.label}</span>
          <span className="ml-auto rounded bg-secondary px-1.5 text-[10px] text-muted-foreground">
            {node.count}
          </span>
        </div>
        {open && (
          <>
            {node.children.map((c) => renderBranch(c, depth + 1))}
            {node.leaves.map((l) => renderLeaf(l, depth + 1))}
          </>
        )}
      </div>
    );
  };

  const renderLeaf = (leaf: TreeLeaf, depth: number) => {
    const selected = leaf.flowId === selectedId;
    return (
      <div
        key={leaf.key}
        onClick={() => select(leaf.flowId)}
        className={cn(
          "flex cursor-pointer items-center gap-2 py-1 pr-2 text-xs",
          selected ? "bg-primary/15" : "hover:bg-accent",
        )}
        style={{ paddingLeft: 8 + depth * 14 + 18 }}
      >
        <MethodBadge method={leaf.method} />
        <StatusBadge status={leaf.status} />
      </div>
    );
  };

  return (
    <div className="min-h-0 flex-1 overflow-auto">
      {tree.map((h) => renderBranch(h, 0))}
    </div>
  );
}
