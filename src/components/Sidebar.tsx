import { Antenna, PanelLeftClose, PanelLeftOpen, Radio, type LucideIcon } from "lucide-react";
import { useLayout, type Mode } from "../layout";
import { cn } from "@/lib/utils";

interface ModeItem {
  id: Mode;
  label: string;
  icon: LucideIcon;
}

const MODES: ModeItem[] = [{ id: "traffic", label: "Traffic capture", icon: Radio }];

export function Sidebar() {
  const mode = useLayout((s) => s.mode);
  const setMode = useLayout((s) => s.setMode);
  const collapsed = useLayout((s) => s.sidebarCollapsed);
  const toggle = useLayout((s) => s.toggleSidebar);

  return (
    <aside
      className={cn(
        "flex shrink-0 flex-col border-r border-border bg-card transition-[width] duration-150",
        collapsed ? "w-14" : "w-52",
      )}
    >
      <div className="flex h-11 items-center gap-2 border-b border-border px-3 font-semibold">
        <Antenna className="size-4 shrink-0 text-primary" />
        {!collapsed && <span className="truncate text-sm">Trawl</span>}
      </div>

      <nav className="flex flex-1 flex-col gap-1 p-2">
        {MODES.map(({ id, label, icon: Icon }) => {
          const active = mode === id;
          return (
            <button
              key={id}
              onClick={() => setMode(id)}
              title={collapsed ? label : undefined}
              className={cn(
                "flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm font-medium transition-colors",
                collapsed && "justify-center px-0",
                active
                  ? "bg-accent text-accent-foreground"
                  : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
              )}
            >
              <Icon className="size-4 shrink-0" />
              {!collapsed && <span className="truncate">{label}</span>}
            </button>
          );
        })}
      </nav>

      <button
        onClick={toggle}
        title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        className={cn(
          "flex items-center gap-2.5 border-t border-border px-3 py-2.5 text-xs font-medium text-muted-foreground transition-colors hover:text-foreground",
          collapsed && "justify-center px-0",
        )}
      >
        {collapsed ? (
          <PanelLeftOpen className="size-4 shrink-0" />
        ) : (
          <>
            <PanelLeftClose className="size-4 shrink-0" />
            <span>Collapse</span>
          </>
        )}
      </button>
    </aside>
  );
}
