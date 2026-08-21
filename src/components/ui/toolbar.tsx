import { useEffect, useLayoutEffect, useRef, useState, type ComponentType } from "react";
import { MoreHorizontal } from "lucide-react";
import { Button } from "./button";
import { cn } from "@/lib/utils";

export interface ToolbarItem {
  id: string;
  label: string;
  icon?: ComponentType<{ className?: string }>;
  onClick: () => void;
  disabled?: boolean;
  title?: string;
  /** Higher stays visible longer. Equal priorities give way right to left. */
  priority?: number;
}

export interface Fit {
  visible: number[];
  overflow: number[];
}

/**
 * Decide what fits.
 *
 * The overflow button costs width of its own, so it is part of the budget —
 * leaving it out makes the boundary flutter: hide one, the next fits, show it
 * again. A container of zero means nothing has been measured yet, and showing
 * everything is the honest first paint.
 */
export function fitItems(
  widths: number[],
  containerWidth: number,
  menuWidth: number,
  priorities: number[],
): Fit {
  const all = widths.map((_, i) => i);
  if (widths.length === 0) return { visible: [], overflow: [] };
  if (containerWidth <= 0) return { visible: all, overflow: [] };

  const total = widths.reduce((sum, x) => sum + x, 0);
  if (total <= containerWidth) return { visible: all, overflow: [] };

  // Least important first; among equals the rightmost goes first, because the
  // left of a toolbar is where the eye starts.
  const order = [...all].sort(
    (a, b) => (priorities[a] ?? 0) - (priorities[b] ?? 0) || b - a,
  );

  const hidden = new Set<number>();
  let used = total;
  const budget = containerWidth - menuWidth;
  for (const index of order) {
    if (used <= budget) break;
    hidden.add(index);
    used -= widths[index];
  }

  return {
    visible: all.filter((i) => !hidden.has(i)),
    overflow: all.filter((i) => hidden.has(i)),
  };
}

/** Buttons that do not fit move into a "⋯" menu instead of being clipped. */
export function Toolbar({ items, className }: { items: ToolbarItem[]; className?: string }) {
  const row = useRef<HTMLDivElement>(null);
  const measure = useRef<HTMLDivElement>(null);
  const [widths, setWidths] = useState<number[]>([]);
  const [available, setAvailable] = useState(0);
  const [open, setOpen] = useState(false);

  // Labels carry counts ("undocumented (50)"), so a re-measure follows them.
  const signature = items.map((i) => `${i.id}:${i.label}`).join("|");

  useLayoutEffect(() => {
    const node = measure.current;
    if (!node) return;
    // gap-1 is 4px, and every button but the first carries one.
    setWidths([...node.children].map((c, i) => (c as HTMLElement).offsetWidth + (i === 0 ? 0 : 4)));
  }, [signature]);

  useEffect(() => {
    const node = row.current;
    if (!node) return;
    const observer = new ResizeObserver(([entry]) => setAvailable(entry.contentRect.width));
    observer.observe(node);
    setAvailable(node.clientWidth);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!open) return;
    const close = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    const away = () => setOpen(false);
    window.addEventListener("keydown", close);
    window.addEventListener("click", away);
    return () => {
      window.removeEventListener("keydown", close);
      window.removeEventListener("click", away);
    };
  }, [open]);

  const fit =
    widths.length === items.length
      ? fitItems(widths, available, 36, items.map((i) => i.priority ?? 0))
      : { visible: items.map((_, i) => i), overflow: [] };

  const render = (item: ToolbarItem) => (
    <Button
      key={item.id}
      variant="outline"
      size="sm"
      title={item.title ?? item.label}
      disabled={item.disabled}
      onClick={item.onClick}
    >
      {item.icon && <item.icon />}
      {item.label}
    </Button>
  );

  return (
    // `w-full` and `overflow-hidden` are what make the measurement honest: the
    // row then reports the width it was *given*, not the width its content
    // would like. Without them a row that spills past its parent measures as
    // if it fitted, and the menu appears far too late — or never.
    <div
      ref={row}
      className={cn(
        "relative flex w-full min-w-0 items-center justify-end gap-1 overflow-hidden",
        className,
      )}
    >
      {/* Off-screen twin: the real row is what the user sees, this is what the
          measurements come from, so hiding a button never changes its width. */}
      <div
        ref={measure}
        aria-hidden
        className="pointer-events-none absolute -left-[9999px] top-0 flex items-center gap-1"
      >
        {items.map(render)}
      </div>

      {fit.visible.map((i) => render(items[i]))}

      {fit.overflow.length > 0 && (
        <div className="relative" onClick={(e) => e.stopPropagation()}>
          <Button
            variant="outline"
            size="sm"
            title={`${fit.overflow.length} more`}
            onClick={() => setOpen((v) => !v)}
          >
            <MoreHorizontal />
          </Button>
          {open && (
            <div className="absolute right-0 z-30 mt-1 min-w-44 rounded-md border border-border bg-popover py-1 shadow-lg">
              {fit.overflow.map((i) => {
                const item = items[i];
                return (
                  <button
                    key={item.id}
                    disabled={item.disabled}
                    title={item.title ?? item.label}
                    onClick={() => {
                      setOpen(false);
                      item.onClick();
                    }}
                    className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-accent disabled:opacity-40"
                  >
                    {item.icon && <item.icon />}
                    {item.label}
                  </button>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
