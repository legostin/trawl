import {
  Antenna,
  Blocks,
  PanelLeftClose,
  PanelLeftOpen,
  PlugZap,
  Puzzle,
  Radio,
  type LucideIcon,
} from "lucide-react";
import { useLayout, type Mode } from "../layout";
import { usePlugins } from "../plugins";
import { cn } from "@/lib/utils";

type IconType = LucideIcon | React.ComponentType<{ className?: string }>;

interface ModeItem {
  id: Mode;
  label: string;
  icon: IconType;
}

const BUILTIN: ModeItem[] = [{ id: "traffic", label: "Traffic capture", icon: Radio }];

export function Sidebar() {
  const mode = useLayout((s) => s.mode);
  const setMode = useLayout((s) => s.setMode);
  const collapsed = useLayout((s) => s.sidebarCollapsed);
  const toggle = useLayout((s) => s.toggleSidebar);
  const pluginModes = usePlugins((s) => s.modes);
  const updateCount = usePlugins((s) => Object.keys(s.updates).length);

  const modes: ModeItem[] = [
    ...BUILTIN,
    ...pluginModes.map((m) => ({ id: m.id, label: m.label, icon: m.icon ?? Puzzle })),
  ];

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
        {modes.map((m) => (
          <NavItem key={m.id} item={m} active={mode === m.id} collapsed={collapsed} onClick={() => setMode(m.id)} />
        ))}
      </nav>

      <div className="flex flex-col gap-1 border-t border-border p-2">
        <NavItem
          item={{ id: "setup", label: "Setup", icon: PlugZap }}
          active={mode === "setup"}
          collapsed={collapsed}
          onClick={() => setMode("setup")}
        />
        <NavItem
          item={{ id: "plugins", label: "Plugins", icon: Blocks }}
          active={mode === "plugins"}
          collapsed={collapsed}
          badge={updateCount > 0}
          onClick={() => setMode("plugins")}
        />
      </div>

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

function NavItem({
  item,
  active,
  collapsed,
  badge,
  onClick,
}: {
  item: ModeItem;
  active: boolean;
  collapsed: boolean;
  badge?: boolean;
  onClick: () => void;
}) {
  const Icon = item.icon;
  return (
    <button
      onClick={onClick}
      title={collapsed ? item.label : undefined}
      className={cn(
        "relative flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm font-medium transition-colors",
        collapsed && "justify-center px-0",
        active
          ? "bg-accent text-accent-foreground"
          : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
      )}
    >
      <span className="relative shrink-0">
        <Icon className="size-4" />
        {badge && (
          <span className="absolute -right-1 -top-1 size-2 rounded-full bg-http-amber ring-2 ring-card" />
        )}
      </span>
      {!collapsed && <span className="truncate">{item.label}</span>}
      {badge && !collapsed && <span className="ml-auto size-2 rounded-full bg-http-amber" />}
    </button>
  );
}
