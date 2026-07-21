import { useMemo, useRef, useState } from "react";
import { ChevronDown, Search, Star, Trash2 } from "lucide-react";
import { useSnippets, type SnippetItem, type SnippetKind } from "../scripting/snippetStore";
import { Button } from "./ui/button";

/** A searchable dropdown of templates or snippets, most-used first, with preview. */
export function SnippetMenu({
  kind,
  label,
  onPick,
}: {
  kind: SnippetKind;
  label: string;
  /** Called with the chosen item's code (usage is recorded automatically). */
  onPick: (code: string) => void;
}) {
  const items = useSnippets((s) => s.items(kind));
  const usage = useSnippets((s) => s.usage);
  const recordUse = useSnippets((s) => s.recordUse);
  const remove = useSnippets((s) => s.remove);
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [preview, setPreview] = useState<SnippetItem | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  const filtered = useMemo(() => {
    const n = q.trim().toLowerCase();
    return n
      ? items.filter((i) => i.label.toLowerCase().includes(n) || i.code.toLowerCase().includes(n))
      : items;
  }, [items, q]);

  const mostUsed = useMemo(
    () => items.filter((i) => (usage[i.id] ?? 0) > 0).slice(0, 3),
    [items, usage],
  );

  const pick = (item: SnippetItem) => {
    onPick(item.code);
    recordUse(item.id);
    setOpen(false);
    setQ("");
  };

  const openMenu = () => {
    setOpen((o) => !o);
    setTimeout(() => searchRef.current?.focus(), 0);
  };

  return (
    <div className="relative">
      <Button size="sm" variant="outline" className="h-6 text-[11px]" onClick={openMenu}>
        {label}
        <ChevronDown className="opacity-60" />
      </Button>
      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />
          <div className="absolute left-0 z-20 mt-1 flex w-[440px] rounded-md border border-border bg-popover shadow-lg">
            {/* list */}
            <div className="flex w-56 shrink-0 flex-col border-r border-border">
              <div className="relative border-b border-border p-1.5">
                <Search className="pointer-events-none absolute left-3 top-1/2 size-3 -translate-y-1/2 text-muted-foreground" />
                <input
                  ref={searchRef}
                  value={q}
                  onChange={(e) => setQ(e.target.value)}
                  placeholder={`Search ${kind}s…`}
                  className="h-6 w-full rounded border border-border bg-background pl-6 pr-2 text-[11px] outline-none focus:border-primary"
                />
              </div>
              <div className="max-h-72 min-h-0 flex-1 overflow-auto p-1">
                {!q && mostUsed.length > 0 && (
                  <>
                    <MenuLabel>Most used</MenuLabel>
                    {mostUsed.map((i) => (
                      <Row key={"mu-" + i.id} item={i} count={usage[i.id]} onPick={() => pick(i)} onHover={() => setPreview(i)} onRemove={remove} />
                    ))}
                    <MenuLabel>All</MenuLabel>
                  </>
                )}
                {filtered.length === 0 ? (
                  <div className="p-2 text-[11px] text-muted-foreground">Nothing found.</div>
                ) : (
                  filtered.map((i) => (
                    <Row key={i.id} item={i} count={usage[i.id]} onPick={() => pick(i)} onHover={() => setPreview(i)} onRemove={remove} />
                  ))
                )}
              </div>
            </div>
            {/* preview */}
            <div className="min-w-0 flex-1 p-2">
              <div className="mb-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
                Preview
              </div>
              <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded bg-muted/40 p-2 font-mono text-[11px] leading-snug text-foreground">
                {(preview ?? filtered[0])?.code ?? "Hover an item to preview."}
              </pre>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

function MenuLabel({ children }: { children: React.ReactNode }) {
  return (
    <div className="px-2 pb-0.5 pt-1.5 text-[9px] font-semibold uppercase tracking-wide text-muted-foreground">
      {children}
    </div>
  );
}

function Row({
  item,
  count,
  onPick,
  onHover,
  onRemove,
}: {
  item: SnippetItem;
  count?: number;
  onPick: () => void;
  onHover: () => void;
  onRemove: (id: string) => void;
}) {
  return (
    <div className="group flex items-center gap-1 rounded hover:bg-accent" onMouseEnter={onHover}>
      <button onClick={onPick} className="flex min-w-0 flex-1 items-center gap-1.5 px-2 py-1 text-left text-[11px]">
        <span className="min-w-0 flex-1 truncate">{item.label}</span>
        {(count ?? 0) > 0 && (
          <span className="flex shrink-0 items-center gap-0.5 text-[9px] text-muted-foreground">
            <Star className="size-2.5" />
            {count}
          </span>
        )}
        {!item.builtin && <span className="shrink-0 text-[9px] text-primary">custom</span>}
      </button>
      {!item.builtin && (
        <button
          title="Delete"
          onClick={() => onRemove(item.id)}
          className="mr-1 shrink-0 rounded p-0.5 text-muted-foreground opacity-0 hover:text-http-red group-hover:opacity-100"
        >
          <Trash2 className="size-3" />
        </button>
      )}
    </div>
  );
}
